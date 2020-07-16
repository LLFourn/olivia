use crate::{
    core::{EventId, ObservedEvent, PathRef},
    db::{self, Db},
    oracle,
};
use core::str::FromStr;
use std::{convert::Infallible, sync::Arc};
use warp::{self, http, Filter};

#[derive(Debug)]
struct DbError;

impl warp::reject::Reject for DbError {}

#[derive(Debug)]
struct NotAnEvent;

impl warp::reject::Reject for NotAnEvent {}

#[derive(Debug, Serialize, Deserialize)]
pub struct PathResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_keys: Option<oracle::OraclePubkeys>,
    pub events: Vec<EventId>,
    pub children: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorMessage {
    code: u16,
    error: String,
}

pub mod filters {
    use super::*;

    pub fn with_db(
        db: Arc<dyn Db>,
    ) -> impl Filter<Extract = (Arc<dyn Db>,), Error = std::convert::Infallible> + Clone {
        warp::any().map(move || db.clone())
    }

    pub fn get_event(
        db: Arc<dyn Db>,
    ) -> impl Filter<Extract = (ObservedEvent,), Error = warp::reject::Rejection> + Clone {
        warp::path::tail()
            .and_then(async move |tail: warp::filters::path::Tail| {
                match EventId::from_str(tail.as_str()) {
                    Ok(event_id) => Ok(event_id),
                    Err(_) => Err(warp::reject::custom(NotAnEvent)),
                }
            })
            .and(with_db(db))
            .and_then(async move |event_id: EventId, db: Arc<dyn Db>| {
                let res = db.get_event(&event_id).await;
                match res {
                    Ok(Some(event)) => Ok(event),
                    Ok(None) => Err(warp::reject::not_found()),
                    Err(_e) => Err(warp::reject::custom(DbError)),
                }
            })
    }

    pub fn get_path(
        db: Arc<dyn Db>,
    ) -> impl Filter<Extract = (db::Item,), Error = warp::reject::Rejection> + Clone {
        warp::path::tail().and(with_db(db)).and_then(
            async move |tail: warp::filters::path::Tail, db: Arc<dyn Db>| {
                let tail = tail.as_str().strip_suffix('/').unwrap_or(tail.as_str());
                let res = db.get_node(tail).await;
                match res {
                    Ok(Some(event)) => Ok(event),
                    Ok(None) => Err(warp::reject::not_found()),
                    Err(_e) => Err(warp::reject::custom(DbError)),
                }
            },
        )
    }

    pub fn get_public_keys(
        db: Arc<dyn Db>,
    ) -> impl Filter<Extract = (oracle::OraclePubkeys,), Error = warp::reject::Rejection> + Clone
    {
        with_db(db).and_then(async move |db: Arc<dyn Db>| {
            db.get_public_keys()
                .await
                .map_err(|_e| warp::reject::custom(DbError))
                .and_then(|opt| opt.ok_or(warp::reject::not_found()))
        })
    }

    pub fn get_root(
        db: Arc<dyn Db>,
    ) -> impl Filter<Extract = (Vec<String>, oracle::OraclePubkeys), Error = warp::reject::Rejection>
           + Clone {
        let get_children = with_db(db.clone()).and_then(async move |db: Arc<dyn Db>| {
            let res = db.get_node(PathRef::root().as_str()).await;
            match res {
                Ok(Some(item)) => Ok(item.children),
                _ => Err(warp::reject::custom(DbError)),
            }
        });

        get_children.and(get_public_keys(db.clone()))
    }
}

pub fn routes(
    db: Arc<dyn Db>,
) -> impl Filter<Extract = impl warp::Reply, Error = Infallible> + Clone {
    let event = warp::get()
        .and(filters::get_event(db.clone()))
        .map(|event: ObservedEvent| warp::reply::json(&event));
    let root = warp::path::end()
        .and(filters::get_root(db.clone()))
        .map(|children, public_keys| {
            warp::reply::json(&PathResponse {
                public_keys: Some(public_keys),
                events: vec![],
                children,
            })
        });

    let path = warp::get()
        .and(filters::get_path(db.clone()))
        .map(|item: db::Item| {
            warp::reply::json(&PathResponse {
                public_keys: None,
                events: item.events.into_iter().map(Into::into).collect(),
                children: item.children,
            })
        });

    root.or(event).or(path).recover(handle_rejection)
}

async fn handle_rejection(err: warp::Rejection) -> Result<impl warp::Reply, Infallible> {
    // This sucks see: https://github.com/seanmonstar/warp/issues/451
    let code;
    let message = None;
    if let Some(DbError) = err.find() {
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
    use crate::{
        core::{EventId, ObservedEvent},
        db::Db,
    };
    use serde_json::from_slice as j;
    use std::sync::Arc;

    #[tokio::test]
    async fn get_path() {
        let db: Arc<dyn Db> = Arc::new(crate::db::in_memory::InMemory::default());
        let event_id = EventId::from_str("test/one/two/3.occur").unwrap();
        let node = event_id.node();
        let obs_event = ObservedEvent::test_new(&event_id);
        let routes = routes(db.clone());

        {
            let res = warp::test::request()
                .path(&format!("/{}", node))
                .reply(&routes)
                .await;

            assert_eq!(res.status(), 404);
            let body = j::<ErrorMessage>(&res.body()).expect("returns an error body");
            assert_eq!(
                body.error,
                http::StatusCode::NOT_FOUND.canonical_reason().unwrap()
            );
        }

        db.insert_event(obs_event.clone()).await.unwrap();

        for path in &[format!("/{}", node), format!("/{}/", node)] {
            let res = warp::test::request().path(path).reply(&routes).await;

            assert_eq!(res.status(), 200);
            let body = j::<PathResponse>(&res.body()).unwrap();
            assert_eq!(body.events, [event_id.clone()]);
        }

        db.insert_event(ObservedEvent::test_new(
            &EventId::from_str("test/one/two/4.occur").unwrap(),
        ))
        .await
        .unwrap();

        let res = warp::test::request()
            .path(&format!("/{}", node.parent().unwrap()))
            .reply(&routes)
            .await;
        let body = j::<PathResponse>(&res.body()).unwrap();
        assert_eq!(body.children, ["test/one/two/3", "test/one/two/4"]);
    }

    //TODO: test get event
    #[tokio::test]
    async fn get_root() {
        let db: Arc<dyn Db> = Arc::new(crate::db::in_memory::InMemory::default());
        let pubkeys =
            crate::keychain::KeyChain::new(crate::seed::Seed::new([42u8; 64])).oracle_pubkeys();
        db.set_public_keys(pubkeys.clone()).await.unwrap();
        let obs_event =
            ObservedEvent::test_new(&EventId::from_str("test/one/two/three.occur").unwrap());

        db.insert_event(obs_event.clone()).await.unwrap();

        let routes = routes(db);

        let res = warp::test::request().path("/").reply(&routes).await;
        assert_eq!(res.status(), 200);
        let body = j::<PathResponse>(&res.body()).unwrap();
        assert_eq!(body.children, ["test"]);
        assert_eq!(body.public_keys, Some(pubkeys));
    }
}
