use crate::db::DbRead;
use core::str::FromStr;
use futures::Future;
use olivia_core::{http::*, Children, EventId, Group, PathNode, PathRef};
use serde::Serialize;
use std::{convert::Infallible, marker::PhantomData, sync::Arc};
use warp::{self, http, Filter};

#[derive(Clone, Debug)]
pub enum ApiReply<T> {
    Ok(T),
    Err(ErrorMessage),
}

impl<T> ApiReply<T> {
    pub async fn map<U, F: FnOnce(T) -> Fut, Fut: Future<Output = U>>(self, op: F) -> ApiReply<U> {
        use ApiReply::*;
        match self {
            Ok(t) => Ok(op(t).await),
            Err(e) => Err(e),
        }
    }

    pub async fn and_then<U, F: FnOnce(T) -> Fut, Fut: Future<Output = ApiReply<U>>>(
        self,
        op: F,
    ) -> ApiReply<U> {
        use ApiReply::*;
        match self {
            Ok(t) => op(t).await,
            Err(e) => Err(e),
        }
    }
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

#[derive(Debug, Default, Clone)]
pub struct Filters<C> {
    curve: PhantomData<C>,
}

impl<C: Group> Filters<C> {
    pub fn with_db(
        &self,
        db: Arc<dyn DbRead<C>>,
    ) -> impl Filter<Extract = (Arc<dyn DbRead<C>>,), Error = std::convert::Infallible> + Clone
    {
        warp::any().map(move || db.clone())
    }

    pub fn get_event(
        &self,
        db: Arc<dyn DbRead<C>>,
    ) -> impl Filter<Extract = (ApiReply<EventResponse<C>>,), Error = warp::reject::Rejection> + Clone
    {
        warp::path::tail()
            .and_then(
                async move |tail: warp::filters::path::Tail| -> Result<ApiReply<EventId>, warp::reject::Rejection> {
                    let path = format!("/{}", tail.as_str());
                    // Need to reject if it doesn't have an event ( have a '.' in last segment )
                    let _ = PathRef::from(path.as_str()).strip_event().ok_or(warp::reject())?;
                    let reply = match EventId::from_str(path.as_str()) {
                        Ok(event_id) => ApiReply::Ok(event_id),
                        Err(_e) => { ApiReply::Err(
                            ErrorMessage::bad_request().with_message("unable to parse as event id"),
                        ) },
                    };

                    Ok(reply)
                },
            )
            .and(self.with_db(db))
            .and_then(
                async move |event_id: ApiReply<EventId>, db: Arc<dyn DbRead<C>>| {
                    let reply = event_id
                        .and_then(async move |event_id| {
                            let res = db.get_event(&event_id).await;
                            match res {
                                Ok(Some(event)) => ApiReply::Ok(event.into()),
                                Ok(None) => ApiReply::Err(ErrorMessage::not_found()),
                                Err(_e) => ApiReply::Err(ErrorMessage::internal_server_error()),
                            }
                        })
                        .await;

                    Ok::<_, Infallible>(reply)
                },
            )
    }

    pub fn get_path(
        &self,
        db: Arc<dyn DbRead<C>>,
    ) -> impl Filter<Extract = (ApiReply<PathResponse>,), Error = Infallible> + Clone {
        warp::path::tail().and(self.with_db(db)).and_then(
            async move |tail: warp::filters::path::Tail, db: Arc<dyn DbRead<C>>| {
                let tail = tail.as_str().strip_suffix('/').unwrap_or(tail.as_str());
                let path = &format!("/{}", tail);
                let node = db.get_node(&path).await;
                let reply = match node {
                    Ok(Some(node)) => ApiReply::Ok(PathResponse {
                        node: PathNode {
                            events: node.events,
                            children: Children {
                                description: node.child_desc,
                            },
                        },
                    }),
                    Ok(None) => ApiReply::Err(ErrorMessage::not_found()),
                    Err(_e) => ApiReply::Err(ErrorMessage::internal_server_error()),
                };
                Ok::<_, Infallible>(reply)
            },
        )
    }

    pub fn get_root(
        &self,
        db: Arc<dyn DbRead<C>>,
    ) -> impl Filter<Extract = (ApiReply<RootResponse<C>>,), Error = Infallible> + Clone {
        self.with_db(db.clone())
            .and_then(async move |db: Arc<dyn DbRead<C>>| {
                let public_keys = db.get_public_keys().await;
                let res = db.get_node(PathRef::root().as_str()).await;

                let reply = if let Ok(Some(public_keys)) = public_keys {
                    if let Ok(Some(node)) = res {
                        ApiReply::Ok(RootResponse {
                            public_keys,
                            node: PathNode {
                                events: node.events,
                                children: Children {
                                    description: node.child_desc,
                                },
                            },
                        })
                    } else {
                        ApiReply::Err(ErrorMessage::internal_server_error())
                    }
                } else {
                    ApiReply::Err(ErrorMessage::internal_server_error())
                };

                Ok::<_, Infallible>(reply)
            })
    }
}

pub fn routes<C: Group>(
    db: Arc<dyn DbRead<C>>,
    _logger: slog::Logger,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::reject::Rejection> + Clone {
    let filters = Filters::<C>::default();
    let event = warp::get().and(filters.get_event(db.clone()));
    let root = warp::path::end().and(filters.get_root(db.clone()));

    let path = warp::get().and(filters.get_path(db.clone()));
    let cors = warp::cors()
        .allow_any_origin()
        .allow_methods(vec!["OPTIONS", "GET", "POST", "DELETE", "PUT"])
        .allow_headers(vec!["content-type"]);

    root.or(event).or(path)
        .with(cors)
}
