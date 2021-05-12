use super::{
    schema::{self, announcements, attestations, events, tree},
    AnnouncedEvent, Attestation, Event, MetaRow, Node, PublicKeyMeta,
};
use crate::{
    core::{self, EventId, OracleKeys},
    curve::*,
    db,
};
use async_trait::async_trait;
use diesel::{
    associations::HasTable, pg::PgConnection, result::Error as DieselError, Connection,
    ExpressionMethods, Insertable, JoinOnDsl, QueryDsl, RunQueryDsl,
};
use std::sync::{Arc, Mutex};

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
impl crate::db::DbRead<SchnorrImpl> for PgBackend {
    async fn get_event(
        &self,
        event_id: &EventId,
    ) -> Result<Option<core::AnnouncedEvent<SchnorrImpl>>, db::Error> {
        let db_mutex = self.conn.clone();
        let event_id = event_id.clone();

        tokio::task::spawn_blocking(move || {
            use schema::events::dsl::*;
            let db = &*db_mutex.lock().unwrap();

            let observed_event = events::table()
                .find(event_id.as_str())
                .inner_join(announcements::table)
                .left_outer_join(attestations::table)
                .first::<AnnouncedEvent>(db);

            match observed_event {
                Err(DieselError::NotFound) => Ok(None),
                res => Ok(Some(res?.into())),
            }
        })
        .await?
    }
    async fn get_node(&self, node: &str) -> Result<Option<db::Item>, db::Error> {
        let node = node.to_string();
        let db_mutex = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let db = &*db_mutex.lock().unwrap();

            let (children, events) = {
                let children = {
                    tree::table
                        .filter(tree::dsl::parent.eq(node.as_str()))
                        .select(tree::dsl::id)
                        .limit(100) //TODO: figure out what to do here
                        .get_results(db)?
                };

                let events = {
                    events::table
                        .filter(events::dsl::node.eq(node.as_str()))
                        .select(events::dsl::id)
                        .get_results::<EventId>(db)?
                };
                (children, events)
            };

            if events.is_empty() && children.is_empty() {
                Ok(None)
            } else {
                Ok(Some(db::Item { children, events }))
            }
        })
        .await?
    }
}

#[async_trait]
impl crate::db::DbWrite<SchnorrImpl> for PgBackend {
    async fn insert_event(
        &self,
        obs_event: core::AnnouncedEvent<SchnorrImpl>,
    ) -> Result<(), db::Error> {
        let node = obs_event.event.id.path();
        let parents = std::iter::successors(Some(node), |parent| (*parent).parent());

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
                let AnnouncedEvent {
                    event,
                    announcement,
                    attestation,
                } = obs_event.into();
                use schema::{
                    announcements::dsl::*, attestations::dsl::*, events::dsl::*, tree::dsl::*,
                };

                nodes
                    .insert_into(tree::table())
                    .on_conflict_do_nothing()
                    .execute(db)?;
                event.insert_into(events::table()).execute(db)?;
                announcement
                    .insert_into(announcements::table())
                    .execute(db)?;

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
        attestation: core::Attestation<SchnorrImpl>,
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
    async fn latest_time_event(&self) -> Result<Option<core::Event>, crate::db::Error> {
        let db_mutex = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            use schema::tree::dsl::*;
            let db = &*db_mutex.lock().unwrap();

            let event = tree::table()
                .filter(parent.eq("/time"))
                .inner_join(events::table)
                .select(events::all_columns)
                .order(events::dsl::expected_outcome_time.desc())
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
    ) -> Result<Option<core::Event>, crate::db::Error> {
        let db_mutex = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let db = &*db_mutex.lock().unwrap();
            use schema::{attestations::columns::event_id, tree::dsl::*};
            let event = tree::table()
                .filter(parent.eq("/time"))
                .inner_join(events::table)
                .left_outer_join(attestations::dsl::attestations.on(event_id.eq(events::dsl::id)))
                .filter(event_id.is_null())
                .order(events::dsl::expected_outcome_time.asc())
                .select(events::all_columns)
                .first::<Event>(db);

            match event {
                Err(DieselError::NotFound) => Ok(None),
                res => Ok(Some(res?.into())),
            }
        })
        .await?
    }
}

impl crate::db::Db<SchnorrImpl> for PgBackend {}

#[async_trait]
impl db::DbMeta<SchnorrImpl> for PgBackend {
    async fn get_public_keys(&self) -> Result<Option<OracleKeys<SchnorrImpl>>, db::Error> {
        use schema::meta::dsl::*;
        let db_mutex = self.conn.clone();
        tokio::task::spawn_blocking(
            move || -> Result<Option<OracleKeys<SchnorrImpl>>, db::Error> {
                let db = &*db_mutex.lock().unwrap();
                let meta_row = meta.find("public_keys").first::<MetaRow>(db);
                match meta_row {
                    Err(DieselError::NotFound) => Ok(None),
                    Err(e) => Err(e.into()),
                    Ok(meta_row) => Ok(Some(
                        serde_json::from_value::<PublicKeyMeta>(meta_row.value)?.public_keys,
                    )),
                }
            },
        )
        .await?
    }

    async fn set_public_keys(&self, public_keys: OracleKeys<SchnorrImpl>) -> Result<(), db::Error> {
        use schema::meta::dsl::*;
        let db_mutex = self.conn.clone();
        let meta_value = serde_json::to_value(PublicKeyMeta {
            curve: SchnorrImpl::default(),
            public_keys,
        })?;
        let meta_row = MetaRow {
            key: "public_keys".into(),
            value: meta_value,
        };
        tokio::task::spawn_blocking(move || {
            let db = &*db_mutex.lock().unwrap();
            meta_row.insert_into(meta::table()).execute(db)?;
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
        crate::db::test::test_db(Arc::new(db).as_ref());
    }

    #[test]
    fn kill_postgres() {
        use std::str::FromStr;
        let docker = clients::Cli::default();
        let (db, container) = new_backend!(docker);
        container.stop();
        let db: Arc<dyn crate::db::Db> = Arc::new(db);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let event = core::AnnouncedEvent::test_unattested_instance(
            EventId::from_str("/test/postgres/database_fail.occur")
                .unwrap()
                .into(),
        );

        let res = rt.block_on(db.insert_event(event.clone()));

        assert!(
            res.is_err(),
            "Cannot insert event for database that is offline"
        );

        //TODO: Test for the error or test that it automatically reconnects
    }

    #[tokio::test]
    async fn time_ticker_postgres() {
        use crate::{db::DbWrite, sources::time_ticker};
        let docker = clients::Cli::default();
        let (db, _container) = new_backend!(docker);

        for time_event in time_ticker::test::time_ticker_db_test_data() {
            db.insert_event(time_event).await.unwrap();
        }

        crate::sources::time_ticker::test::test_time_ticker_db(Arc::new(db)).await;
    }

    #[tokio::test]
    async fn postgres_test_against_oracle() {
        let docker = clients::Cli::default();
        let (db, _container) = new_backend!(docker);
        crate::oracle::test::test_oracle_event_lifecycle(Arc::new(db)).await
    }
}
