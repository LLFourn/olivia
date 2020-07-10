use super::{
    schema::{self, attestations, nonces},
    Attestation, Event, MetaRow, Node, ObservedEvent,
};
use crate::{
    db,
    event::{self, EventId, Path, PathRef},
    oracle,
    oracle::OraclePubkeys,
};
use async_trait::async_trait;
use diesel::ExpressionMethods;
use diesel::{
    associations::HasTable, pg::PgConnection, result::Error as DieselError, Connection, Insertable,
    QueryDsl, RunQueryDsl,
};
use std::{
    convert::TryInto,
    sync::{Arc, Mutex},
};

pub struct PgBackend {
    conn: Arc<Mutex<PgConnection>>,
    #[allow(unused_must_use, dead_code)]
    database_url: String,
}

impl PgBackend {
    pub fn connect(database_url: &str) -> diesel::result::ConnectionResult<Self> {
        let conn: PgConnection = PgConnection::establish(database_url)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            database_url: database_url.to_string(),
        })
    }

    pub fn setup(&self) -> Result<(), diesel_migrations::RunMigrationsError> {
        let conn = &*self.conn.lock().unwrap();
        diesel_migrations::run_pending_migrations(conn)
    }
}

#[async_trait]
impl crate::db::DbRead for PgBackend {
    async fn get_event(
        &self,
        event_id: &EventId,
    ) -> Result<Option<event::ObservedEvent>, db::Error> {
        let db_mutex = self.conn.clone();
        let event_id = event_id.clone();

        tokio::task::spawn_blocking(move || {
            use schema::events::dsl::*;
            let db = &*db_mutex.lock().unwrap();

            let observed_event = events::table()
                .find(event_id.as_ref())
                .inner_join(nonces::table)
                .left_outer_join(attestations::table)
                .first::<ObservedEvent>(db);

            match observed_event {
                Err(DieselError::NotFound) => Ok(None),
                res => Ok(Some(res?.into())),
            }
        })
        .await?
    }
    async fn get_path(&self, path: PathRef<'_>) -> Result<Option<db::Item>, db::Error> {
        let path: Path = path.into();
        let event_id: EventId = path.clone().into();

        let event = match path.is_root() {
            true => None,
            false => self.get_event(&event_id).await?,
        };

        let db_mutex = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let db = &*db_mutex.lock().unwrap();
            let mut children = vec![];

            if !path.is_root() {
                use schema::events::dsl::*;
                let event_children = events::table()
                    .filter(parent.eq(path.as_str()))
                    .select(id)
                    // HACK: limit events until we have a way of describing children without listing them all
                    .limit(100)
                    .get_results(db)?;

                children.extend(event_children);
            }

            let child_nodes = {
                use schema::tree::dsl::*;

                match path.is_root() {
                    true => tree::table()
                        .filter(parent.is_null())
                        .select(id)
                        .get_results(db)?,
                    false => tree::table()
                        .filter(parent.eq(path.as_str()))
                        .select(id)
                        .get_results(db)?,
                }
            };

            children.extend(child_nodes);

            if event.is_none() && children.len() == 0 {
                Ok(None)
            } else {
                Ok(Some(db::Item { children, event }))
            }
        })
        .await?
    }
}

#[async_trait]
impl crate::db::DbWrite for PgBackend {
    async fn insert_event(&self, obs_event: event::ObservedEvent) -> Result<(), db::Error> {
        let parent = obs_event.event.id.parent();
        let parents = std::iter::successors(Some(parent), |parent| (*parent).parent());
        let nodes = parents
            .clone()
            .zip(parents.skip(1).map(Some).chain(std::iter::once(None)))
            .map(|(child, parent)| Node {
                id: child.as_str().into(),
                parent: parent.map(|parent| parent.as_str().into()),
            })
            .collect::<Vec<Node>>();

        let db_mutex = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let db = &mut *db_mutex.lock().unwrap();
            db.transaction(|| {
                let ObservedEvent {
                    event,
                    nonce,
                    attestation,
                } = obs_event.into();
                use schema::{attestations::dsl::*, events::dsl::*, nonces::dsl::*, tree::dsl::*};

                nodes
                    .insert_into(tree::table())
                    .on_conflict_do_nothing()
                    .execute(db)?;
                event.insert_into(events::table()).execute(db)?;
                nonce.insert_into(nonces::table()).execute(db)?;

                if let Some(attestation) = attestation {
                    attestation.insert_into(attestations::table()).execute(db)?;
                }

                Ok(())
            })
        })
        .await?
    }

    async fn complete_event(
        &self,
        id: &EventId,
        attestation: event::Attestation,
    ) -> Result<(), db::Error> {
        let db_mutex = self.conn.clone();
        let id = id.clone();
        tokio::task::spawn_blocking(move || {
            use schema::attestations::dsl::attestations;
            let db = &*db_mutex.lock().unwrap();
            let attestation = Attestation::from_core_domain(id.clone(), attestation);
            attestation.insert_into(attestations::table()).execute(db)?;
            Ok(())
        })
        .await?
    }
}

#[async_trait]
impl crate::db::TimeTickerDb for PgBackend {
    async fn latest_time_event(&self) -> Result<Option<event::Event>, crate::db::Error> {
        let db_mutex = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            use diesel::ExpressionMethods;
            use schema::events::dsl::*;
            let db = &*db_mutex.lock().unwrap();

            let event = events::table()
                .filter(parent.eq("time"))
                .order(expected_outcome_time.desc())
                .first::<Event>(db);

            match event {
                Err(DieselError::NotFound) => Ok(None),
                res => Ok(Some(res?.into())),
            }
        })
        .await?
    }

    async fn earliest_unattested_time_event(
        &self,
    ) -> Result<Option<event::Event>, crate::db::Error> {
        let db_mutex = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let db = &*db_mutex.lock().unwrap();
            use diesel::{ExpressionMethods, Table};
            use schema::{attestations::columns::event_id, events::dsl::*};

            let event = events::table()
                .filter(parent.eq("time"))
                .left_outer_join(attestations::table)
                .filter(event_id.is_null())
                .order(expected_outcome_time.asc())
                .select(events::all_columns())
                .first::<Event>(db);

            match event {
                Err(DieselError::NotFound) => Ok(None),
                res => Ok(Some(res?.into())),
            }
        })
        .await?
    }
}

impl crate::db::Db for PgBackend {}

#[async_trait]
impl db::DbMeta for PgBackend {
    async fn get_public_keys(&self) -> Result<Option<oracle::OraclePubkeys>, db::Error> {
        use schema::meta::dsl::*;
        let db_mutex = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let db = &*db_mutex.lock().unwrap();
            let pubkeys = meta.find("oracle_pubkeys").first::<MetaRow>(db);

            match pubkeys {
                Err(DieselError::NotFound) => Ok(None),
                res => Ok(Some(res?.try_into()?)),
            }
        })
        .await?
    }

    async fn set_public_keys(&self, public_keys: OraclePubkeys) -> Result<(), db::Error> {
        use schema::meta::dsl::*;
        let db_mutex = self.conn.clone();
        let meta_value: MetaRow = public_keys.into();
        tokio::task::spawn_blocking(move || {
            let db = &*db_mutex.lock().unwrap();
            meta_value.insert_into(meta::table()).execute(db)?;
            Ok(())
        })
        .await?
    }
}

#[cfg(all(test, feature = "docker_tests"))]
mod test {
    use super::*;
    use testcontainers::{clients, images, Docker};

    macro_rules! new_backend {
        ($docker:expr) => {{
            let container = $docker.run(images::postgres::Postgres::default());
            let url = format!(
                "postgres://postgres@localhost:{}",
                container.get_host_port(5432).unwrap()
            );

            let db = PgBackend::connect(&url).unwrap();
            assert!(db.setup().is_ok());

            (db, container)
        }};
    }

    #[test]
    fn generic_test_postgres() {
        let docker = clients::Cli::default();
        let (db, _container) = new_backend!(docker);
        crate::db::test::test_db(Arc::new(db));
    }

    #[test]
    fn kill_postgres() {
        let docker = clients::Cli::default();
        let (db, container) = new_backend!(docker);
        container.stop();
        let db: Arc<dyn crate::db::Db> = Arc::new(db);
        let mut rt = tokio::runtime::Runtime::new().unwrap();
        let event = event::ObservedEvent::test_new(&EventId::from(
            "/test/postgres/database_fail".to_string(),
        ));

        let res = rt.block_on(db.insert_event(event.clone()));

        assert!(
            res.is_err(),
            "Cannot insert event for database that is offline"
        );

        //TODO: Test for the error or test that it automatically reconnects
    }

    #[test]
    fn time_ticker_postgres() {
        use crate::{db::DbWrite, sources::time_ticker};
        let docker = clients::Cli::default();
        let (db, _container) = new_backend!(docker);
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        for time_event in time_ticker::test::time_ticker_db_test_data() {
            rt.block_on(db.insert_event(time_event)).unwrap();
        }

        crate::sources::time_ticker::test::test_time_ticker_db(Arc::new(db));
    }
}
