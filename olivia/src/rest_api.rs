use crate::{
    core::{
        http::{EventResponse, PathResponse},
        EventId, PathRef, Schnorr,
    },
    db::{self, Db},
};
use core::str::FromStr;
use std::{convert::Infallible, marker::PhantomData, sync::Arc};
use warp::{self, http, Filter};

#[derive(Debug)]
struct DbError(crate::db::Error);

impl warp::reject::Reject for DbError {}

#[derive(Debug)]
struct NotAnEvent;

impl warp::reject::Reject for NotAnEvent {}

#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorMessage {
    code: u16,
    error: String,
}

#[derive(Debug, Default, Clone)]
pub struct Filters<C> {
    curve: PhantomData<C>,
}

impl<C: Schnorr> Filters<C> {
    pub fn with_db(
        &self,
        db: Arc<dyn Db<C>>,
    ) -> impl Filter<Extract = (Arc<dyn Db<C>>,), Error = std::convert::Infallible> + Clone {
        warp::any().map(move || db.clone())
    }

    pub fn get_event(
        &self,
        db: Arc<dyn Db<C>>,
    ) -> impl Filter<Extract = (EventResponse<C>,), Error = warp::reject::Rejection> + Clone {
        warp::path::tail()
            .and(warp::query::raw())
            .and_then(
                async move |tail: warp::filters::path::Tail, query: String| {
                    let id = format!("/{}?{}", tail.as_str(), query);
                    match EventId::from_str(&id) {
                        Ok(event_id) => Ok(event_id),
                        Err(_) => Err(warp::reject::custom(NotAnEvent)),
                    }
                },
            )
            .and(self.with_db(db))
            .and_then(async move |event_id: EventId, db: Arc<dyn Db<C>>| {
                let res = db.get_event(&event_id).await;
                match res {
                    Ok(Some(event)) => Ok(event.into()),
                    Ok(None) => Err(warp::reject::not_found()),
                    Err(e) => Err(warp::reject::custom(DbError(e))),
                }
            })
    }

    pub fn get_path(
        &self,
        db: Arc<dyn Db<C>>,
    ) -> impl Filter<Extract = (db::Item,), Error = warp::reject::Rejection> + Clone {
        warp::path::tail().and(self.with_db(db)).and_then(
            async move |tail: warp::filters::path::Tail, db: Arc<dyn Db<C>>| {
                let tail = tail.as_str().strip_suffix('/').unwrap_or(tail.as_str());
                let path = &format!("/{}", tail);
                let res = db.get_node(&path).await;
                match res {
                    Ok(Some(event)) => Ok(event),
                    Ok(None) => Err(warp::reject::not_found()),
                    Err(e) => Err(warp::reject::custom(DbError(e))),
                }
            },
        )
    }

    pub fn get_public_key(
        &self,
        db: Arc<dyn Db<C>>,
    ) -> impl Filter<Extract = (C::PublicKey,), Error = warp::reject::Rejection> + Clone {
        self.with_db(db).and_then(async move |db: Arc<dyn Db<C>>| {
            db.get_public_key()
                .await
                .map_err(|e| warp::reject::custom(DbError(e)))
                .and_then(|opt| opt.ok_or(warp::reject::not_found()))
        })
    }

    pub fn get_root(
        &self,
        db: Arc<dyn Db<C>>,
    ) -> impl Filter<Extract = (Vec<String>, C::PublicKey), Error = warp::reject::Rejection> + Clone
    {
        let get_children = self
            .with_db(db.clone())
            .and_then(async move |db: Arc<dyn Db<C>>| {
                let res = db.get_node(PathRef::root().as_str()).await;
                match res {
                    Ok(Some(item)) => Ok(item.children),
                    Ok(None) => Err(warp::reject::not_found()),
                    Err(e) => Err(warp::reject::custom(DbError(e))),
                }
            });

        get_children.and(self.get_public_key(db.clone()))
    }
}

pub fn routes<C: Schnorr>(
    db: Arc<dyn Db<C>>,
    logger: slog::Logger,
) -> impl Filter<Extract = impl warp::Reply, Error = Infallible> + Clone {
    let filters = Filters::<C>::default();
    let event = warp::get()
        .and(filters.get_event(db.clone()))
        .map(|event: EventResponse<C>| warp::reply::json(&event));
    let root = warp::path::end()
        .and(filters.get_root(db.clone()))
        .map(|children, public_keys| {
            warp::reply::json(&PathResponse::<C> {
                public_key: Some(public_keys),
                events: vec![],
                children,
            })
        });

    let path = warp::get()
        .and(filters.get_path(db.clone()))
        .map(|item: db::Item| {
            warp::reply::json(&PathResponse::<C> {
                public_key: None,
                events: item.events.into_iter().map(Into::into).collect(),
                children: item.children,
            })
        });

    root.or(event).or(path).recover(move |err| handle_rejection(err, logger.clone()))
}

async fn handle_rejection(err: warp::Rejection, logger: slog::Logger) -> Result<impl warp::Reply, Infallible> {
    // This sucks see: https://github.com/seanmonstar/warp/issues/451
    let code;
    let message = None;
    if let Some(DbError(e)) = err.find() {
        error!(logger, "DB error"; "error" => format!("{}",e));
        code = http::StatusCode::INTERNAL_SERVER_ERROR;
    } else if err.is_not_found() {
        code = http::StatusCode::NOT_FOUND;
    } else if let Some(NotAnEvent) = err.find() {
        code = http::StatusCode::NOT_FOUND;
    } else {
        code = http::StatusCode::BAD_REQUEST;
    }

    let json = warp::reply::json(&ErrorMessage {
        code: code.as_u16(),
        error: message.unwrap_or(code.canonical_reason().unwrap()).into(),
    });

    Ok(warp::reply::with_status(json, code))
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
            let logger = slog::Logger::root(slog::Discard,o!());
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

            assert_eq!(res.status(), 404);
            let body = j::<ErrorMessage>(&res.body()).expect("returns an error body");
            assert_eq!(
                body.error,
                http::StatusCode::NOT_FOUND.canonical_reason().unwrap()
            );
        }

        oracle.add_event(event_id.clone().into()).await.unwrap();

        for path in &[format!("{}", node), format!("{}/", node)] {
            let res = warp::test::request().path(path).reply(&routes).await;

            assert_eq!(res.status(), 200);
            let body = j::<PathResponse<SchnorrImpl>>(&res.body()).unwrap();
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
        let body = j::<PathResponse<SchnorrImpl>>(&res.body()).unwrap();
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
        let body = j::<PathResponse<SchnorrImpl>>(&res.body()).unwrap();
        assert_eq!(body.children, ["/test"]);
        assert_eq!(body.public_key, Some(oracle.public_key()));
    }

    #[tokio::test]
    async fn get_event() {
        let (oracle, routes) = setup!();
        let event_id = EventId::from_str("/test/one/two/three?occur").unwrap();

        oracle
            .add_event(event_id.clone().clone().into())
            .await
            .unwrap();

        let public_key = {
            let root = warp::test::request().path("/").reply(&routes).await;
            j::<PathResponse<SchnorrImpl>>(&root.body())
                .unwrap()
                .public_key
                .unwrap()
        };

        let res = warp::test::request()
            .path(event_id.as_str())
            .reply(&routes)
            .await;

        let body = j::<EventResponse<SchnorrImpl>>(&res.body()).unwrap();
        assert_eq!(body.id, event_id);

        assert!(body.announcement.verify(&event_id, &public_key))
    }
}
