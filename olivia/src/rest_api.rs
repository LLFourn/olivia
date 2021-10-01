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

async fn get_event<C: Group>(
    tail: warp::filters::path::Tail,
    db: Arc<dyn DbReadOracle<C>>,
) -> Result<ApiReply<EventResponse<C>>, warp::reject::Rejection> {
    let tail = tail.as_str().strip_suffix('/').unwrap_or(tail.as_str());
    let path = format!("/{}", tail);
    let path = match Path::from_str(&path) {
        Ok(path) => path,
        Err(_) => {
            return Ok(ApiReply::Err(
                ErrorMessage::bad_request()
                    .with_message(format!("'{}' is not a valid event path", path)),
            ))
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
            Err(_) | Ok(None) => ApiReply::Err(ErrorMessage::internal_server_error()),
        },
        Err(_) | Ok(None) => ApiReply::Err(ErrorMessage::internal_server_error()),
    }
}

async fn get_path<C: Group>(
    tail: warp::filters::path::Tail,
    db: Arc<dyn DbReadOracle<C>>,
) -> ApiReply<PathResponse> {
    let tail = tail.as_str().strip_suffix('/').unwrap_or(tail.as_str());
    let path = match Path::from_str(&format!("/{}", tail)) {
        Ok(path) => path,
        Err(_) => {
            return ApiReply::Err(
                ErrorMessage::bad_request()
                    .with_message(format!("'/{}' is not a valid event path", tail)),
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

// impl<C: Group> Filters<C> {

//
//     pub async fn get_root(db: Arc<dyn DbReadOracle<C>>) -> ApiReply<RootResponse<C>> {
//         let public_keys = db.get_public_keys().await;
//         let res = db.get_node(PathRef::root()).await;

//         let reply = if let Ok(Some(public_keys)) = public_keys {
//             if let Ok(Some(node)) = res {
//                 ApiReply::Ok(RootResponse {
//                     public_keys,
//                     node: GetPath {
//                         events: node.events,
//                         child_desc: node.child_desc,
//                     },
//                 })
//             } else {
//                 ApiReply::Err(ErrorMessage::internal_server_error())
//             }
//         } else {
//             ApiReply::Err(ErrorMessage::internal_server_error())
//         };

//         reply
//     }
//     pub async fn get_root(db: Arc<dyn DbReadOracle<C>>) -> ApiReply<RootResponse<C>> {
//         let public_keys = db.get_public_keys().await;
//         let res = db.get_node(PathRef::root()).await;

//         let reply = if let Ok(Some(public_keys)) = public_keys {
//             if let Ok(Some(node)) = res {
//                 ApiReply::Ok(RootResponse {
//                     public_keys,
//                     node: GetPath {
//                         events: node.events,
//                         child_desc: node.child_desc,
//                     },
//                 })
//             } else {
//                 ApiReply::Err(ErrorMessage::internal_server_error())
//             }
//         } else {
//             ApiReply::Err(ErrorMessage::internal_server_error())
//         };

//         reply
//     }
// }

pub fn routes<C: Group>(
    db: Arc<dyn DbReadOracle<C>>,
    _logger: slog::Logger,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::reject::Rejection> + Clone {
    let event = warp::get()
        .and(warp::path::tail())
        .and(with_db(db.clone()))
        .and_then(get_event);
    let root = warp::get()
        .and(warp::path::end())
        .and(with_db(db.clone()))
        .and_then(|db| async { Ok::<_, Infallible>(get_root(db).await) });
    let path = warp::get()
        .and(warp::path::tail())
        .and(with_db(db.clone()))
        .and_then(|tail, db| async { Ok::<_, Infallible>(get_path(tail, db).await) });

    let cors = warp::cors()
        .allow_any_origin()
        .allow_methods(vec!["OPTIONS", "GET", "POST", "DELETE", "PUT"])
        .allow_headers(vec!["content-type"]);

    root.or(event).or(path).with(cors)
}
