use crate::{
    db::Db,
    event::{EventId, ObservedEvent},
    oracle,
};
use std::sync::Arc;
use warp::{self, Filter};

#[derive(Debug)]
struct DbError;

impl warp::reject::Reject for DbError {}

#[derive(Debug, Serialize, Deserialize)]
pub struct Meta {
    pub public_keys: oracle::OraclePubkeys,
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
        warp::path::tail().and(with_db(db)).and_then(
            async move |tail: warp::filters::path::Tail, db: Arc<dyn Db>| {
                let event_id = EventId::from(tail.as_str().to_string());
                let res = db.get_event(&event_id).await;
                match res {
                    Ok(Some(obs_event)) => Ok(obs_event),
                    Ok(None) => Err(warp::reject::not_found()),
                    Err(_e) => Err(warp::reject::custom(DbError)),
                }
            },
        )
    }

    pub fn get_meta(
        db: Arc<dyn Db>,
    ) -> impl Filter<Extract = (Meta,), Error = warp::reject::Rejection> + Clone {
        with_db(db).and_then(async move |db: Arc<dyn Db>| {
            db.get_public_keys()
                .await
                .map(Option::unwrap)
                .map(|public_keys| Meta { public_keys })
                .map_err(|_e| warp::reject::custom(DbError))
        })
    }
}

pub fn routes(
    db: Arc<dyn Db>,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    let root = warp::path::end()
        .and(filters::get_meta(db.clone()))
        .map(|meta| warp::reply::json(&meta));

    let event = warp::any()
        .and(filters::get_event(db.clone()))
        .map(|event: ObservedEvent| warp::reply::json(&event));

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
    async fn get_event() {
        let db: Arc<dyn Db> = Arc::new(crate::db::in_memory::InMemory::default());
        let path = "test/one/two/three";
        let obs_event = ObservedEvent::test_new(&EventId::from(path.to_string()));
        let filter = filters::get_event(db.clone());

        assert!(warp::test::request()
            .path(&format!("/{}", path))
            .filter(&filter)
            .await
            .is_err());

        db.insert_event(obs_event.clone()).await.unwrap();

        assert_eq!(
            warp::test::request()
                .path(&format!("/{}", path))
                .filter(&filter)
                .await
                .unwrap(),
            obs_event
        );
    }
}

// pub mod handlers {
//     use super::*;

//     pub async fn get_event(
//         tail: String,
//         db: Arc<dyn Db>,
//     ) -> Result<Option<ObservedEvent>, crate::db::DbError> {
//         db.get_event.await
//     }
// }
