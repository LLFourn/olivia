use super::NodeKind;
use crate::db::*;
use async_trait::async_trait;
use olivia_core::{
    AnnouncedEvent, Attestation, Child, ChildDesc, Event, EventId, Group, OracleKeys, PathRef,
    RawAnnouncement, RawOracleEvent,
};
use std::iter::once;
use tokio::sync::RwLock;
use tokio_postgres::{types::*, NoTls, Transaction};

pub async fn connect_read(database_url: &str) -> anyhow::Result<tokio_postgres::Client> {
    let (client, connection) = tokio_postgres::connect(database_url, NoTls).await?;

    // The connection object performs the actual communication with the database,
    // so spawn it off to run on its own.
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("connection error: {}", e);
        }
    });

    Ok(client)
}

#[derive(Clone, Debug)]
struct Ltree(String);
#[derive(Clone, Debug)]
struct Lquery(String);

impl ToSql for Ltree {
    fn to_sql(
        &self,
        ty: &Type,
        out: &mut bytes::BytesMut,
    ) -> Result<IsNull, Box<dyn std::error::Error + Sync + Send>> {
        use bytes::BufMut;
        // put the ltree version as the first byte
        out.put_u8(1);
        self.0.to_sql(ty, out)
    }

    fn accepts(ty: &Type) -> bool {
        ty.name() == "ltree"
    }

    to_sql_checked!();
}

impl ToSql for Lquery {
    fn to_sql(
        &self,
        ty: &Type,
        out: &mut bytes::BytesMut,
    ) -> Result<IsNull, Box<dyn std::error::Error + Sync + Send>> {
        use bytes::BufMut;
        // put the ltree version as the first byte
        out.put_u8(1);
        self.0.to_sql(ty, out)
    }

    fn accepts(ty: &Type) -> bool {
        ty.name() == "lquery"
    }

    to_sql_checked!();
}

impl<'a> From<PathRef<'a>> for Ltree {
    fn from(path: PathRef<'a>) -> Self {
        Ltree(
            path.as_str()[1..]
                .replace(|c: char| !c.is_ascii_alphanumeric() && c != '/', "__")
                .replace("/", "."),
        )
    }
}

impl Lquery {
    pub fn ends_with(p: PathRef<'_>) -> Lquery {
        let mut query = Ltree::from(p).0;
        query.insert_str(0, "*.");
        Lquery(query)
    }

    pub fn everything() -> Lquery {
        Lquery("*".into())
    }
}

pub struct PgBackendWrite {
    client: RwLock<tokio_postgres::Client>,
    #[allow(dead_code)]
    database_url: String,
}

impl PgBackendWrite {
    pub async fn connect(database_url: &str) -> anyhow::Result<Self> {
        let (client, connection) = tokio_postgres::connect(database_url, NoTls).await?;

        // The connection object performs the actual communication with the database,
        // so spawn it off to run on its own.
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("connection error: {}", e);
            }
        });

        Ok(PgBackendWrite {
            client: RwLock::new(client),
            database_url: database_url.into(),
        })
    }

    pub async fn setup(&self) -> anyhow::Result<()> {
        let sql = include_str!("postgres/init.sql");
        Ok(self.client.read().await.batch_execute(sql).await?)
    }
}

#[async_trait]
impl<C: Group> crate::db::DbReadOracle<C> for tokio_postgres::Client {
    async fn get_announced_event(&self, id: &EventId) -> Result<Option<AnnouncedEvent<C>>, Error> {
        let row = self
            .query_opt(
                r#"SELECT id,
                      expected_outcome_time,
                      (ann).oracle_event,
                      (ann).signature,
                      (att).outcome,
                      (att).scalars,
                      (att).time
               FROM event
                 WHERE event.id = $1
            "#,
                &[&id.as_str()],
            )
            .await?;

        match row {
            None => return Ok(None),
            Some(row) => {
                let attestation = row.try_get("outcome").ok().map(|outcome| Attestation {
                    outcome,
                    scalars: row.get("scalars"),
                    time: row.get("time"),
                });
                Ok(Some(AnnouncedEvent {
                    event: Event {
                        id: row.get("id"),
                        expected_outcome_time: row.get("expected_outcome_time"),
                    },
                    announcement: RawAnnouncement {
                        oracle_event: RawOracleEvent::from_json_bytes(row.get("oracle_event")),
                        signature: row.get("signature"),
                    },
                    attestation,
                }))
            }
        }
    }

    async fn get_public_keys(&self) -> Result<Option<olivia_core::OracleKeys<C>>, Error> {
        let row = self
            .query_opt(r#"SELECT value FROM meta WHERE key = 'public_keys'"#, &[])
            .await?;

        Ok(row
            .map(|row| serde_json::from_value(row.get("value")))
            .transpose()?)
    }
}

#[async_trait]
impl crate::db::DbReadEvent for tokio_postgres::Client {
    async fn get_node(&self, path: PathRef<'_>) -> Result<Option<GetPath>, Error> {
        let row = self
            .query_opt(r#"SELECT kind FROM tree WHERE id = $1"#, &[&path.as_str()])
            .await?;

        let trim = |x: String| {
            x.trim_start_matches(path.as_str())
                .trim_start_matches('/')
                .to_string()
        };

        let child_desc = match row {
            None => return Ok(None),
            Some(row) => {
                let kind = row
                    .get::<_, Option<_>>("kind")
                    .map(serde_json::from_value)
                    .transpose()?;
                match kind {
                    None | Some(NodeKind::List) => {
                        let rows = self
                            .query(
                                r"SELECT id, kind FROM tree WHERE parent = $1 LIMIT 100",
                                &[&path.as_str()],
                            )
                            .await?;
                        ChildDesc::List {
                            list: rows
                                .into_iter()
                                .map(|row| Child {
                                    name: trim(row.get("id")),
                                    kind: row
                                        .get::<_, Option<_>>("kind")
                                        .map(|json| serde_json::from_value(json).unwrap())
                                        .unwrap_or(NodeKind::List),
                                })
                                .collect(),
                        }
                    }
                    Some(NodeKind::Range { range_kind }) => {
                        let rows = self
                            .query(
                                r"( SELECT id, kind FROM tree WHERE parent = $1 ORDER BY id ASC LIMIT 1 )
                                  UNION ALL
                                  ( SELECT id, kind FROM tree WHERE parent = $1 ORDER BY id DESC LIMIT 1 )",
                                &[&path.as_str()],
                            )
                            .await?;

                        let mut min_max_children = rows
                            .into_iter()
                            .map(|row| Child {
                                name: trim(row.get("id")),
                                kind: row
                                    .get::<_, Option<_>>("kind")
                                    .map(|json| serde_json::from_value(json).unwrap())
                                    .unwrap_or(NodeKind::List),
                            })
                            .collect::<Vec<_>>();

                        let end = min_max_children.pop();
                        let start = min_max_children.pop();

                        ChildDesc::Range {
                            start,
                            range_kind,
                            end,
                        }
                    }
                }
            }
        };

        let events = self
            .query(
                r#"SELECT id FROM event WHERE path = $1"#,
                &[&Ltree::from(path)],
            )
            .await?
            .into_iter()
            .map(|row| row.get::<_, EventId>("id").event_kind())
            .collect();

        Ok(Some(GetPath { events, child_desc }))
    }

    async fn query_event(&self, query: EventQuery<'_, '_>) -> anyhow::Result<Option<Event>> {
        let EventQuery {
            path,
            attested,
            order,
            ends_with,
            ref kind,
        } = query;
        let row = self
            .query_opt(
                format!(
                    r#"SELECT event.id, expected_outcome_time FROM event
                   WHERE $1 @> path
                     AND path ~ $2
                     {}
                     {}
                   ORDER BY expected_outcome_time {} LIMIT 1"#,
                    match kind {
                        Some(kind) => format!("AND event.id LIKE ('%{}')", kind),
                        None => "".to_string(),
                    },
                    match attested {
                        Some(true) => "AND (att).outcome IS NOT NULL",
                        Some(false) => "AND (att).outcome IS NULL",
                        None => "",
                    },
                    match order {
                        Order::Earliest => "ASC",
                        Order::Latest => "DESC",
                    }
                )
                .as_str(),
                &[
                    &Ltree::from(path.unwrap_or(PathRef::root())),
                    &ends_with
                        .map(|ends_with| Lquery::ends_with(ends_with))
                        .unwrap_or(Lquery::everything()),
                ],
            )
            .await?;

        Ok(row.map(|row| Event {
            id: row.get("id"),
            expected_outcome_time: row.get("expected_outcome_time"),
        }))
    }
}

#[async_trait]
impl<C: Group> crate::db::DbReadOracle<C> for PgBackendWrite {
    async fn get_announced_event(&self, id: &EventId) -> Result<Option<AnnouncedEvent<C>>, Error> {
        self.client.read().await.get_announced_event(id).await
    }

    async fn get_public_keys(&self) -> Result<Option<olivia_core::OracleKeys<C>>, Error> {
        self.client.read().await.get_public_keys().await
    }
}

#[async_trait]
impl crate::db::DbReadEvent for PgBackendWrite {
    async fn get_node(&self, path: PathRef<'_>) -> Result<Option<GetPath>, Error> {
        DbReadEvent::get_node(&*self.client.read().await, path).await
    }

    async fn query_event(&self, query: EventQuery<'_, '_>) -> anyhow::Result<Option<Event>> {
        DbReadEvent::query_event(&*self.client.read().await, query).await
    }
}

impl PgBackendWrite {
    async fn set_node_parents(
        &self,
        tx: &Transaction<'_>,
        node: PathRef<'_>,
    ) -> anyhow::Result<()> {
        let children = std::iter::successors(Some(node), |parent| (*parent).parent())
            .map(|p| p.to_string())
            .collect::<Vec<_>>();
        let parents = children
            .clone()
            .into_iter()
            .skip(1)
            .map(Some)
            .chain(once(None))
            .collect::<Vec<_>>();

        let params = children
            .iter()
            .zip(parents.iter())
            .flat_map(|(child, parent)| {
                once(child as &(dyn ToSql + Sync)).chain(once(parent as &(dyn ToSql + Sync)))
            })
            .collect::<Vec<_>>();

        let values = (1..=children.len())
            .map(|i| format!("(${},${})", i * 2 - 1, i * 2))
            .collect::<Vec<_>>()
            .join(",");

        tx.execute(
            format!(
                "INSERT INTO tree (id, parent) VALUES {} ON CONFLICT DO NOTHING",
                values
            )
            .as_str(),
            &params[..],
        )
        .await?;
        Ok(())
    }
}

#[async_trait]
impl<C: Group> crate::db::DbWrite<C> for PgBackendWrite {
    async fn insert_event(&self, event: AnnouncedEvent<C>) -> Result<(), Error> {
        let mut client = self.client.write().await;
        let tx = client.transaction().await?;
        let node = event.event.id.path();
        self.set_node_parents(&tx, node).await?;

        tx.execute(
            "INSERT INTO event (id, expected_outcome_time, ann, path) VALUES ($1,$2,ROW($3,$4), $5)",
            &[
                &event.event.id.as_str(),
                &event.event.expected_outcome_time,
                &event.announcement.oracle_event.as_bytes(),
                &event.announcement.signature,
                &Ltree::from(event.event.id.path())
            ],
        )
        .await?;

        if let Some(Attestation {
            outcome,
            scalars,
            time,
        }) = event.attestation
        {
            tx.execute(
                "UPDATE event SET att.outcome = $2, att.time = $3, att.scalars = $4 WHERE id = $1",
                &[&event.event.id.as_str(), &outcome, &time, &scalars],
            )
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn complete_event(
        &self,
        event_id: &EventId,
        attestation: Attestation<C>,
    ) -> Result<(), Error> {
        self.client
            .read()
            .await
            .execute(
                "UPDATE event SET att.outcome = $2, att.time = $3, att.scalars = $4 WHERE id = $1",
                &[
                    &event_id.as_str(),
                    &attestation.outcome,
                    &attestation.time,
                    &attestation.scalars,
                ],
            )
            .await?;

        Ok(())
    }

    async fn set_public_keys(&self, public_keys: OracleKeys<C>) -> Result<(), Error> {
        let value = serde_json::to_value(public_keys).unwrap();
        let key = "public_keys";
        self.client
            .read()
            .await
            .execute(
                "INSERT INTO meta (key,value) VALUES ($1, $2)",
                &[&key, &value],
            )
            .await?;
        Ok(())
    }

    async fn set_node(&self, node: Node) -> anyhow::Result<()> {
        let kind_json = serde_json::to_value(&node.kind).unwrap();
        let mut client = self.client.write().await;
        let tx = client.transaction().await?;
        self.set_node_parents(&tx, node.path.as_path_ref()).await?;
        tx.execute(
            r#"UPDATE tree SET kind = $1 WHERE id = $2"#,
            &[&kind_json, &node.path.as_str()],
        )
        .await?;

        tx.commit().await?;
        Ok(())
    }
}

impl<C: Group> Db<C> for PgBackendWrite {}

impl<C: Group> BorrowDb<C> for PgBackendWrite {
    fn borrow_db(&self) -> &dyn Db<C> {
        self
    }
}

#[cfg(test)]
#[allow(unused_macros)]
macro_rules! new_backend {
    ($docker:expr) => {{
        let container = $docker.run(images::postgres::Postgres::default().with_version(13));
        let url = format!(
            "postgres://postgres@localhost:{}",
            container.get_host_port(5432).unwrap()
        );

        (url, container)
    }};
}

#[cfg(all(test, feature = "docker_tests"))]
crate::run_time_db_tests! {
    db => db,
    event_db => event_db,
    curve => olivia_secp256k1::Secp256k1,
    {
        use testcontainers::{clients, images, Docker};
        use crate::db::postgres::*;
        use std::sync::Arc;
        let docker = clients::Cli::default();
        let (url, _container) = new_backend!(docker);
        let db = PgBackendWrite::connect(&url).await.unwrap();
        db.setup().await.unwrap();
        let db: Arc<dyn Db<olivia_secp256k1::Secp256k1>> =  Arc::new(db);
        let event_db: Arc<dyn DbReadEvent> = Arc::new(connect_read(&url).await.unwrap());
    }
}

#[cfg(all(test, feature = "docker_tests"))]
crate::run_rest_api_tests! {
    oracle => oracle,
    routes => routes,
    curve => olivia_secp256k1::Secp256k1,
    {
        use testcontainers::{clients, images, Docker};
        use std::sync::Arc;
        let docker = clients::Cli::default();
        let (url, _container) = new_backend!(docker);
        let db_oracle = PgBackendWrite::connect(&url).await.unwrap();
        db_oracle.setup().await.unwrap();
        let http_db = connect_read(&url).await.unwrap();
        let oracle = crate::oracle::Oracle::<olivia_secp256k1::Secp256k1>::new(crate::seed::Seed::new([42u8; 64]), Arc::new(db_oracle)).await.unwrap();
        let routes = crate::rest_api::routes::<olivia_secp256k1::Secp256k1>(Arc::new(http_db), slog::Logger::root(slog::Discard, o!()));
    }
}

#[cfg(all(test, feature = "docker_tests"))]
crate::run_node_db_tests! {
    db => db,
    curve => olivia_secp256k1::Secp256k1,
    {
        use testcontainers::{clients, images, Docker};
        use std::sync::Arc;
        let docker = clients::Cli::default();
        let (url, _container) = new_backend!(docker);
        let db = PgBackendWrite::connect(&url).await.unwrap();
        db.setup().await.unwrap();
        let db: Arc<dyn Db<olivia_secp256k1::Secp256k1>> = Arc::new(db);
    }
}

#[cfg(all(test, feature = "docker_tests"))]
crate::run_query_db_tests! {
    db => db,
    curve => olivia_secp256k1::Secp256k1,
    {
        use testcontainers::{clients, images, Docker};
        use std::sync::Arc;
        let docker = clients::Cli::default();
        let (url, _container) = new_backend!(docker);
        let db = PgBackendWrite::connect(&url).await.unwrap();
        db.setup().await.unwrap();
        let db: Arc<dyn Db<olivia_secp256k1::Secp256k1>> = Arc::new(db);
    }
}

#[cfg(all(test, feature = "docker_tests"))]
mod test {
    use super::*;
    use std::sync::Arc;
    use testcontainers::{clients, images, Docker};

    #[tokio::test]
    async fn kill_postgres() {
        use std::str::FromStr;
        let docker = clients::Cli::default();
        let (url, container) = new_backend!(docker);
        let db = PgBackendWrite::connect(&url).await.unwrap();
        container.stop();
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        let db: Arc<dyn crate::db::Db<olivia_secp256k1::Secp256k1>> = Arc::new(db);
        let event = olivia_core::AnnouncedEvent::test_unattested_instance(
            EventId::from_str("/test/postgres/database_fail.occur")
                .unwrap()
                .into(),
        );

        let res = db.insert_event(event.clone()).await;

        assert!(
            res.is_err(),
            "Cannot insert event for database that is offline"
        );

        //TODO: Test for the error or test that it automatically reconnects
    }

    #[tokio::test]
    async fn postgres_test_against_oracle() {
        let docker = clients::Cli::default();
        let (url, _container) = new_backend!(docker);
        let db = PgBackendWrite::connect(&url).await.unwrap();
        db.setup().await.unwrap();
        crate::oracle::test::test_oracle_event_lifecycle::<olivia_secp256k1::Secp256k1>(Arc::new(
            db,
        ))
        .await
    }
}
