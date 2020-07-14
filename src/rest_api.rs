use crate::{
    core::{EventId, ObservedEvent, PathRef},
    db::{self, Db},
    oracle,
};
use core::str::FromStr;
use std::sync::Arc;
use warp::{self, Filter};

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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<String>,
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
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
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

    root.or(event).or(path)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        core::{EventId, ObservedEvent},
        db::Db,
    };
    use std::sync::Arc;

    #[tokio::test]
    async fn get_path() {
        let db: Arc<dyn Db> = Arc::new(crate::db::in_memory::InMemory::default());
        let event_id = EventId::from_str("test/one/two/three.occur").unwrap();
        let node = event_id.node();
        let obs_event = ObservedEvent::test_new(&event_id);
        let filter = filters::get_path(db.clone());

        assert!(warp::test::request()
            .path(&format!("/{}", node))
            .filter(&filter)
            .await
            .is_err());

        db.insert_event(obs_event.clone()).await.unwrap();

        let item = warp::test::request()
            .path(&format!("/{}", node))
            .filter(&filter)
            .await
            .unwrap();

        assert_eq!(item.events, [event_id.clone()]);

        let item = warp::test::request()
            .path(&format!("/{}/", node))
            .filter(&filter)
            .await
            .unwrap();

        assert_eq!(item.events, [event_id]);
    }

    //TODO: test get event

    // #[tokio::test]
    // async fn get_root() {
    //     let db: Arc<dyn Db> = Arc::new(crate::db::in_memory::InMemory::default());
    //     let path = "test/one/two/three";
    //     let obs_event = ObservedEvent::test_new(&EventId::from(path.to_string()));
    //     db.insert_event(obs_event.clone()).await.unwrap();
    //     let filter = filters::get_root(db.clone());

    //     let root = warp::test::request()
    //         .path("")
    //         .filter(&filter)
    //         .await
    //         .unwrap();
    // }
}
