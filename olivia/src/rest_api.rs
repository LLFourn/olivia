use crate::db::DbReadOracle;
use core::{convert::TryFrom, str::FromStr};
use olivia_core::{http::*, EventId, GetPath, Group, Path, PathRef};
use serde::Serialize;
use std::{convert::Infallible, sync::Arc};
use warp::{self, http, Filter};

#[derive(Clone, Debug)]
pub enum ApiReply<T> {
    Ok(T),
    Err(ErrorMessage),
}

impl<T: Send + Serialize> warp::Reply for ApiReply<T> {
    fn into_response(self) -> warp::reply::Response {
        match self {
            ApiReply::Ok(value) => {
                let reply = warp::reply::json(&value);
                reply.into_response()
            }
            ApiReply::Err(err) => warp::reply::with_status(
                warp::reply::json(&err),
                http::StatusCode::from_u16(err.code).unwrap(),
            )
            .into_response(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ErrorMessage {
    pub code: u16,
    pub error: String,
}

impl ErrorMessage {
    fn not_found() -> Self {
        Self::from_status(http::StatusCode::NOT_FOUND)
    }

    fn internal_server_error() -> Self {
        Self::from_status(http::StatusCode::INTERNAL_SERVER_ERROR)
    }

    fn bad_request() -> Self {
        Self::from_status(http::StatusCode::BAD_REQUEST)
    }

    pub fn from_status(status: http::StatusCode) -> Self {
        Self {
            code: status.as_u16(),
            error: status.canonical_reason().unwrap().into(),
        }
    }

    pub fn with_message<M: Into<String>>(self, message: M) -> Self {
        Self {
            code: self.code,
            error: message.into(),
        }
    }
}

fn with_db<C: Group>(
    db: Arc<dyn DbReadOracle<C>>,
) -> impl Filter<Extract = (Arc<dyn DbReadOracle<C>>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || db.clone())
}

fn percent_decoded_tail(
) -> impl Filter<Extract = (ApiReply<String>,), Error = std::convert::Infallible> + Clone {
    warp::path::tail().map(|tail: warp::path::Tail| {
        match percent_encoding::percent_decode_str(tail.as_str()).decode_utf8() {
            Ok(tail) => ApiReply::Ok(tail.to_string()),
            Err(_) => ApiReply::Err(
                ErrorMessage::bad_request().with_message("'{}' was not URL encoded properly"),
            ),
        }
    })
}

async fn get_event<C: Group>(
    tail: ApiReply<String>,
    query: Option<String>,
    db: Arc<dyn DbReadOracle<C>>,
) -> Result<ApiReply<EventResponse<C>>, warp::reject::Rejection> {
    let tail = match tail {
        ApiReply::Ok(tail) => tail,
        ApiReply::Err(e) => return Ok(ApiReply::Err(e)),
    };
    if tail.ends_with("/") {
        return Err(warp::reject());
    }
    let path = match query {
        Some(query) => format!("/{}?{}", tail, query),
        None => format!("/{}", tail),
    };

    let path = match Path::from_str(&path) {
        Ok(path) => path,
        Err(e) => {
            return Ok(ApiReply::Err(ErrorMessage::bad_request().with_message(
                format!("'{}' is not a valid event path: {}", path, e),
            )))
        }
    };

    // if we've got a valid path but it doesn't look like an event we should reject
    let _ = path.as_path_ref().strip_event().ok_or(warp::reject())?;

    let reply = match EventId::try_from(path.clone()) {
        Ok(event_id) => {
            let res = db.get_announced_event(&event_id).await;
            match res {
                Ok(Some(event)) => ApiReply::Ok(event.into()),
                Ok(None) => ApiReply::Err(ErrorMessage::not_found()),
                Err(_e) => ApiReply::Err(ErrorMessage::internal_server_error()),
            }
        }
        Err(e) => ApiReply::Err(
            ErrorMessage::bad_request()
                .with_message(format!("'{}' is not a valid event id: {}", path, e)),
        ),
    };

    Ok(reply)
}

pub async fn get_root<C: Group>(db: Arc<dyn DbReadOracle<C>>) -> ApiReply<RootResponse<C>> {
    let public_keys = db.get_public_keys().await;
    match public_keys {
        Ok(Some(public_keys)) => match db.get_node(PathRef::root()).await {
            Ok(Some(node)) => ApiReply::Ok(RootResponse {
                public_keys,
                node: GetPath {
                    events: node.events,
                    child_desc: node.child_desc,
                },
            }),
            Err(_) | Ok(None) => ApiReply::Err(
                ErrorMessage::internal_server_error()
                    .with_message("root node could not be read from the database"),
            ),
        },
        Err(_) | Ok(None) => ApiReply::Err(
            ErrorMessage::internal_server_error()
                .with_message("oracle public keys could not be retrieved from the database"),
        ),
    }
}

async fn get_path<C: Group>(
    tail: ApiReply<String>,
    db: Arc<dyn DbReadOracle<C>>,
) -> ApiReply<PathResponse> {
    let tail = match tail {
        ApiReply::Ok(tail) => tail,
        ApiReply::Err(e) => return ApiReply::Err(e),
    };
    let tail = tail.as_str().strip_suffix('/').unwrap_or(tail.as_str());
    let path = match Path::from_str(&format!("/{}", tail)) {
        Ok(path) => path,
        Err(e) => {
            return ApiReply::Err(
                ErrorMessage::bad_request()
                    .with_message(format!("'/{}' is not a valid event path: {}", tail, e)),
            )
        }
    };
    let node = db.get_node(path.as_path_ref()).await;
    match node {
        Ok(Some(node)) => ApiReply::Ok(PathResponse {
            node: GetPath {
                events: node.events,
                child_desc: node.child_desc,
            },
        }),
        Ok(None) => ApiReply::Err(ErrorMessage::not_found()),
        Err(_e) => ApiReply::Err(ErrorMessage::internal_server_error()),
    }
}

async fn handle_rejection(err: warp::Rejection) -> Result<impl warp::Reply, warp::Rejection> {
    Ok(ApiReply::<()>::Err(
        ErrorMessage::internal_server_error()
            .with_message(format!("unable to recover from {:?}", err)),
    ))
}

pub fn routes<C: Group>(
    db: Arc<dyn DbReadOracle<C>>,
    _logger: slog::Logger,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::reject::Rejection> + Clone {
    let event = warp::get()
        .and(percent_decoded_tail())
        .map(|tail| (tail, None))
        .untuple_one()
        .and(with_db(db.clone()))
        .and_then(get_event);

    let event_with_query = warp::get()
        .and(percent_decoded_tail())
        .and(warp::filters::query::raw().map(|query| Some(query)))
        .and(with_db(db.clone()))
        .and_then(get_event);

    let root = warp::get()
        .and(warp::path::end())
        .and(with_db(db.clone()))
        .and_then(|db| async { Ok::<_, Infallible>(get_root(db).await) });
    let path = warp::get()
        .and(percent_decoded_tail())
        .and(with_db(db.clone()))
        .and_then(|tail, db| async { Ok::<_, Infallible>(get_path(tail, db).await) });

    let cors = warp::cors()
        .allow_any_origin()
        .allow_methods(vec!["OPTIONS", "GET", "POST", "DELETE", "PUT"])
        .allow_headers(vec!["content-type"]);

    root.or(event_with_query)
        .or(event)
        .or(path)
        .with(cors)
        .recover(handle_rejection)
}
