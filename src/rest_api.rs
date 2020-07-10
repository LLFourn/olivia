use crate::{
    db::{self, Db},
    event::ObservedEvent,
    event::PathRef,
    oracle,
};
use std::sync::Arc;
use warp::{self, Filter};

#[derive(Debug)]
struct DbError;

impl warp::reject::Reject for DbError {}

#[derive(Debug, Serialize)]
pub struct Response {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_keys: Option<oracle::OraclePubkeys>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event: Option<ObservedEvent>,
    pub children: Vec<String>,
}

pub mod filters {
    use super::*;

    pub fn with_db(
        db: Arc<dyn Db>,
    ) -> impl Filter<Extract = (Arc<dyn Db>,), Error = std::convert::Infallible> + Clone {
        warp::any().map(move || db.clone())
    }

    pub fn get_path(
        db: Arc<dyn Db>,
    ) -> impl Filter<Extract = (db::Item,), Error = warp::reject::Rejection> + Clone {
        warp::path::tail().and(with_db(db)).and_then(
            async move |tail: warp::filters::path::Tail, db: Arc<dyn Db>| {
                let tail = tail.as_str();
                let tail = if tail.ends_with('/') {
                    &tail[..tail.len() - 1]
                } else {
                    tail
                };
                let path = PathRef::from(tail);
                let res = db.get_path(path).await;
                match res {
                    Ok(Some(item)) => Ok(item),
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
            let res = db.get_path(PathRef::root()).await;
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
    let root = warp::path::end()
        .and(filters::get_root(db.clone()))
        .map(|children, public_keys| {
            warp::reply::json(&Response {
                public_keys: Some(public_keys),
                event: None,
                children,
            })
        });

    let event = warp::any()
        .and(filters::get_path(db.clone()))
        .map(|item: db::Item| {
            warp::reply::json(&Response {
                public_keys: None,
                event: item.event,
                children: item.children,
            })
        });

    root.or(event)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        db::Db,
        event::{EventId, ObservedEvent},
    };
    use std::sync::Arc;

    #[tokio::test]
    async fn get_path() {
        let db: Arc<dyn Db> = Arc::new(crate::db::in_memory::InMemory::default());
        let path = "test/one/two/three";
        let obs_event = ObservedEvent::test_new(&EventId::from(path.to_string()));
        let filter = filters::get_path(db.clone());

        assert!(warp::test::request()
            .path(&format!("/{}", path))
            .filter(&filter)
            .await
            .is_err());

        db.insert_event(obs_event.clone()).await.unwrap();

        let item = warp::test::request()
            .path(&format!("/{}", path))
            .filter(&filter)
            .await
            .unwrap();

        assert_eq!(item.event.unwrap(), obs_event);

        let item = warp::test::request()
            .path(&format!("/{}/", path))
            .filter(&filter)
            .await
            .unwrap();

        assert_eq!(item.event.unwrap(), obs_event);
    }

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
