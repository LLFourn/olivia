use crate::{
    core::{
        http::{EventResponse, PathResponse, RootResponse},
        EventId, PathRef, Group,
    },
    db::Db,
};
use core::str::FromStr;
use futures::Future;
use serde::Serialize;
use std::{convert::Infallible, marker::PhantomData, sync::Arc};
use warp::{self, http, Filter};


#[derive(Clone, Debug)]
pub enum ApiReply<T> {
    Ok(T),
    Err(ErrorMessage),
}


impl<T> ApiReply<T> {
    pub async fn map<U, F: FnOnce(T) -> Fut, Fut: Future<Output=U>>(self, op: F) -> ApiReply<U> {
        use ApiReply::*;
        match self {
            Ok(t) => Ok(op(t).await),
            Err(e) => Err(e),
        }
    }


    pub async fn and_then<U, F: FnOnce(T) -> Fut, Fut: Future<Output=ApiReply<U>>>(self, op: F) -> ApiReply<U> {
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
    code: u16,
    error: String,
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
        db: Arc<dyn Db<C>>,
    ) -> impl Filter<Extract = (Arc<dyn Db<C>>,), Error = std::convert::Infallible> + Clone {
        warp::any().map(move || db.clone())
    }

    pub fn get_event(
        &self,
        db: Arc<dyn Db<C>>,
    ) -> impl Filter<Extract = (ApiReply<EventResponse<C>>,), Error = warp::reject::Rejection> + Clone
    {
        warp::path::tail()
            .and(warp::query::raw())
            .and_then(
                async move |tail: warp::filters::path::Tail, query: String| {
                    let id = format!("/{}?{}", tail.as_str(), query);
                    let reply = match EventId::from_str(&id) {
                        Ok(event_id) => ApiReply::Ok(event_id),
                        Err(_) =>   ApiReply::Err(ErrorMessage::bad_request().with_message("unable to parse as event id")),
                    };

                    Ok::<_ ,Infallible>(reply)
                },
            )
            .and(self.with_db(db))
            .and_then(async move |event_id: ApiReply<EventId>, db: Arc<dyn Db<C>>| {
                let reply = event_id.and_then(async move |event_id| {
                    let res = db.get_event(&event_id).await;
                    match res {
                        Ok(Some(event)) => ApiReply::Ok(event.into()),
                        Ok(None) => ApiReply::Err(ErrorMessage::not_found()),
                        Err(_e) =>  ApiReply::Err(ErrorMessage::internal_server_error())
                    }
                }).await;

                Ok::<_, Infallible>(reply)
            })
    }

    pub fn get_path(
        &self,
        db: Arc<dyn Db<C>>,
    ) -> impl Filter<Extract = (ApiReply<PathResponse>,), Error = Infallible> + Clone
    {
        warp::path::tail().and(self.with_db(db)).and_then(
            async move |tail: warp::filters::path::Tail, db: Arc<dyn Db<C>>| {
                let tail = tail.as_str().strip_suffix('/').unwrap_or(tail.as_str());
                let path = &format!("/{}", tail);
                let node = db.get_node(&path).await;
                let reply = match node {
                    Ok(Some(node)) => ApiReply::Ok(PathResponse {
                        events: node.events,
                        children: node.children,
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
        db: Arc<dyn Db<C>>,
    ) -> impl Filter<Extract = (ApiReply<RootResponse<C>>,), Error = Infallible> + Clone {
        self.with_db(db.clone()).and_then(
            async move |db: Arc<dyn Db<C>>| {
                let public_keys = db.get_public_keys().await;
                let res = db.get_node(PathRef::root().as_str()).await;

                let reply = if let Ok(Some(public_keys)) = public_keys {
                    if let Ok(Some(node)) = res {
                        ApiReply::Ok(RootResponse {
                            public_keys,
                            path_response: PathResponse {
                                events: node.events,
                                children: node.children,
                            }
                        })
                    } else {
                        ApiReply::Err(ErrorMessage::internal_server_error())
                    }
                } else {
                    ApiReply::Err(ErrorMessage::internal_server_error())
                };

                Ok::<_, Infallible>(reply)
            },
        )
    }
}

pub fn routes<C: Group>(
    db: Arc<dyn Db<C>>,
    _logger: slog::Logger,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::reject::Rejection> + Clone {
    let filters = Filters::<C>::default();
    let event = warp::get().and(filters.get_event(db.clone()));
    let root = warp::path::end().and(filters.get_root(db.clone()));

    let path = warp::get().and(filters.get_path(db.clone()));

    root.or(event)
        .or(path)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{core::EventId, curve::SchnorrImpl, db::Db};
    use serde_json::from_slice as j;
    use std::sync::Arc;

    macro_rules! setup {
        () => {{
            let db: Arc<dyn Db<SchnorrImpl>> = Arc::new(crate::db::in_memory::InMemory::default());
            let oracle = crate::oracle::Oracle::new(crate::seed::Seed::new([42u8; 64]), db.clone())
                .await
                .unwrap();
            let logger = slog::Logger::root(slog::Discard, o!());
            (oracle, routes(db, logger))
        }};
    }

    #[tokio::test]
    async fn get_path() {
        let (oracle, routes) = setup!();
        let event_id = EventId::from_str("/test/one/two/3?occur").unwrap();
        let node = event_id.as_path();

        {
            let res = warp::test::request()
                .path(event_id.as_str())
                .reply(&routes)
                .await;

            assert_eq!(res.status(), http::StatusCode::NOT_FOUND);
            let body = j::<ErrorMessage>(&res.body()).expect("returns an error body");
            assert_eq!(
                body.error,
                http::StatusCode::NOT_FOUND.canonical_reason().unwrap()
            );
        }

        oracle.add_event(event_id.clone().into()).await.unwrap();

        assert_eq!(
            warp::test::request()
                .path("/test/one/two/42")
                .reply(&routes)
                .await
                .status(),
            http::StatusCode::NOT_FOUND,
            "similar but non-existing path should 404"
        );

        for path in &[format!("{}", node), format!("{}/", node)] {
            let res = warp::test::request().path(path).reply(&routes).await;

            assert_eq!(res.status(), 200);
            let body = j::<PathResponse>(&res.body()).unwrap();
            assert_eq!(body.events, [event_id.clone()]);
        }

        oracle
            .add_event(EventId::from_str("/test/one/two/4?occur").unwrap().into())
            .await
            .unwrap();

        let res = warp::test::request()
            .path(&format!("{}", node.parent().unwrap()))
            .reply(&routes)
            .await;

        let body = j::<PathResponse>(&res.body()).unwrap();
        assert_eq!(body.children, ["/test/one/two/3", "/test/one/two/4"]);
    }

    #[tokio::test]
    async fn get_root() {
        let (oracle, routes) = setup!();
        oracle
            .add_event(
                EventId::from_str("/test/one/two/three?occur")
                    .unwrap()
                    .into(),
            )
            .await
            .unwrap();

        let res = warp::test::request().path("/").reply(&routes).await;
        assert_eq!(res.status(), 200);
        let body = j::<RootResponse<SchnorrImpl>>(&res.body()).unwrap();
        assert_eq!(body.path_response.children, ["/test"]);
        assert_eq!(body.public_keys, oracle.public_keys());
    }

    #[tokio::test]
    async fn get_event() {
        let (oracle, routes) = setup!();
        let event_id = EventId::from_str("/test/one/two/three?occur").unwrap();

        oracle
            .add_event(event_id.clone().clone().into())
            .await
            .unwrap();

        assert_eq!(
            warp::test::request()
                .path("/test/one/two/four?occur")
                .reply(&routes)
                .await
                .status(),
            http::StatusCode::NOT_FOUND,
            "similar but non-existing event should 404"
        );

        let public_keys = {
            let root = warp::test::request().path("/").reply(&routes).await;
            j::<RootResponse<SchnorrImpl>>(&root.body())
                .unwrap()
                .public_keys
        };

        let res = warp::test::request()
            .path(event_id.as_str())
            .reply(&routes)
            .await;

        let body = j::<EventResponse<SchnorrImpl>>(&res.body()).unwrap();

        assert!(body
            .announcement
            .verify_against_id(&event_id, &public_keys.announcement_key)
            .is_some())
    }
}
