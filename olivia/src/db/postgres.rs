use super::NodeKind;
use crate::db::*;
use async_trait::async_trait;
use olivia_core::{
    attest, chrono::NaiveDate, AnnouncedEvent, Attestation, AttestationSchemes, Child, ChildDesc,
    Event, EventId, Group, OracleKeys, Path, PathRef, PrefixPath, RawAnnouncement, RawOracleEvent,
};
use std::{
    collections::{BTreeMap, HashSet},
    iter::once,
    str::FromStr,
};
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
        if p.is_root() {
            return Lquery("*".into());
        }
        let mut query = Ltree::from(p).0;
        query.insert_str(0, "*.");
        Lquery(query)
    }
}

pub struct PgBackendWrite {
    client: RwLock<tokio_postgres::Client>,
    #[allow(dead_code)]
    database_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Version {
    version: u32,
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

    pub async fn version(&self) -> anyhow::Result<Version> {
        let row = self
            .client
            .read()
            .await
            .query_one(r#"SELECT value FROM meta WHERE key = 'version'"#, &[])
            .await?;
        Ok(serde_json::from_value(
            row.get::<_, serde_json::Value>("value"),
        )?)
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
                      (att).olivia_v1_scalars,
                      (att).ecdsa_v1_signature,
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
                    schemes: AttestationSchemes {
                        olivia_v1: row
                            .get::<_, Option<_>>("olivia_v1_scalars")
                            .map(|scalars| attest::OliviaV1 { scalars }),
                        ecdsa_v1: row
                            .get::<_, Option<_>>("ecdsa_v1_signature")
                            .map(|signature| attest::EcdsaV1 { signature }),
                    },
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

        let child_desc = match row {
            None => return Ok(None),
            Some(row) => {
                let kind = row
                    .get::<_, Option<_>>("kind")
                    .map(serde_json::from_value)
                    .transpose()?
                    .unwrap_or_else(|| olivia_describe::infer_node_kind(path));
                match kind {
                    NodeKind::List => {
                        let rows = self
                            .query(
                                r"SELECT id, kind FROM tree WHERE parent = $1 LIMIT 100",
                                &[&path.as_str()],
                            )
                            .await?;
                        ChildDesc::List {
                            list: rows
                                .into_iter()
                                .map(|row| {
                                    let id = row.get::<_, Path>("id");
                                    let name = id
                                        .clone()
                                        .strip_prefix_path(path)
                                        .as_path_ref()
                                        .first()
                                        .unwrap()
                                        .to_string();
                                    Child {
                                        name,
                                        kind: row
                                            .get::<_, Option<_>>("kind")
                                            .map(|json| serde_json::from_value(json).unwrap())
                                            .unwrap_or_else(|| {
                                                olivia_describe::infer_node_kind(id.as_path_ref())
                                            }),
                                    }
                                })
                                .collect(),
                        }
                    }
                    NodeKind::Range { range_kind } => {
                        let next_unattested = {
                            let next_event = self
                                .query_event(EventQuery {
                                    path: Some(path),
                                    attested: Some(false),
                                    order: Order::Earliest,
                                    ..Default::default()
                                })
                                .await?;

                            next_event.and_then(|event| {
                                Some(
                                    event
                                        .id
                                        .path()
                                        .to_path()
                                        .strip_prefix_path(path)
                                        .as_path_ref()
                                        .segments()
                                        .next()?
                                        .to_string(),
                                )
                            })
                        };
                        let rows = self
                            .query(
                                r"( SELECT id FROM tree WHERE parent = $1 ORDER BY id ASC LIMIT 1 )
                                  UNION ALL
                                  ( SELECT id FROM tree WHERE parent = $1 ORDER BY id DESC LIMIT 1 )",
                                &[&path.as_str()],
                            )
                            .await?;

                        let mut min_max_children = rows
                            .into_iter()
                            .map(|row| {
                                row.get::<_, Path>("id")
                                    .strip_prefix_path(path)
                                    .as_path_ref()
                                    .first()
                                    .unwrap()
                                    .to_string()
                            })
                            .collect::<Vec<_>>();

                        let end = min_max_children.pop();
                        let start = min_max_children.pop();

                        ChildDesc::Range {
                            start,
                            range_kind,
                            next_unattested,
                            end,
                        }
                    }
                    NodeKind::DateMap => {
                        let rows = self
                            .query(
                                r#"SELECT event.id FROM event
                                 WHERE $1 @> path
                            "#,
                                &[&Ltree::from(path)],
                            )
                            .await?;

                        let mut dates = BTreeMap::<NaiveDate, HashSet<String>>::new();

                        for row in rows {
                            let event_id = row.get::<_, EventId>("id").strip_prefix_path(path);
                            let mut segments = event_id.path().segments();
                            if let (Some(date), Some(next)) = (segments.next(), segments.next()) {
                                if let Ok(date) = NaiveDate::from_str(date) {
                                    dates
                                        .entry(date)
                                        .and_modify(|list| {
                                            list.insert(next.to_string());
                                        })
                                        .or_insert_with(move || {
                                            vec![next.to_string()].into_iter().collect()
                                        });
                                }
                            }
                        }

                        ChildDesc::DateMap { dates }
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
                     AND id LIKE $3
                   ORDER BY expected_outcome_time {} LIMIT 1"#,
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
                    &Lquery::ends_with(ends_with),
                    &match kind {
                        Some(kind) => format!("%.{}", kind),
                        None => "%".to_string(),
                    },
                ],
            )
            .await?;

        Ok(row.map(|row| Event {
            id: row.get("id"),
            expected_outcome_time: row.get("expected_outcome_time"),
        }))
    }

    // TODO: DRY this
    async fn query_events(&self, query: EventQuery<'_, '_>) -> anyhow::Result<Vec<Event>> {
        let EventQuery {
            path,
            attested,
            order,
            ends_with,
            ref kind,
        } = query;
        let rows = self
            .query(
                format!(
                    r#"SELECT event.id, expected_outcome_time FROM event
                   WHERE $1 @> path
                     AND path ~ $2
                     {}
                     AND id LIKE $3
                   ORDER BY expected_outcome_time {}"#,
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
                    &Lquery::ends_with(ends_with),
                    &match kind {
                        Some(kind) => format!("%.{}", kind),
                        None => "%".to_string(),
                    },
                ],
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| Event {
                id: row.get("id"),
                expected_outcome_time: row.get("expected_outcome_time"),
            })
            .collect())
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

    async fn query_events(&self, query: EventQuery<'_, '_>) -> anyhow::Result<Vec<Event>> {
        DbReadEvent::query_events(&*self.client.read().await, query).await
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
        let mut tx = client.transaction().await?;
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

        if let Some(attestation) = event.attestation {
            _complete_event(&event.event.id, attestation, &mut tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn complete_event(
        &self,
        event_id: &EventId,
        attestation: Attestation<C>,
    ) -> Result<(), Error> {
        _complete_event(event_id, attestation, &mut *self.client.write().await).await?;
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

async fn _complete_event<Client: tokio_postgres::GenericClient, C: Group>(
    event_id: &EventId,
    attestation: Attestation<C>,
    client: &mut Client,
) -> Result<(), tokio_postgres::Error> {
    let Attestation {
        outcome,
        schemes: AttestationSchemes {
            olivia_v1,
            ecdsa_v1,
        },
        time,
    } = attestation;
    client.execute(
        "UPDATE event SET att.outcome = $2, att.time = $3, att.olivia_v1_scalars= $4, att.ecdsa_v1_signature = $5 WHERE id = $1",
        &[&event_id.as_str(), &outcome, &time, &olivia_v1.map(|x| x.scalars), &ecdsa_v1.map(|x| x.signature)],
    )
          .await?;
    Ok(())
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
        let db = Arc::new(db);
        crate::oracle::test::test_oracle_event_lifecycle::<olivia_secp256k1::Secp256k1>(db.clone())
            .await;
        crate::oracle::test::test_price_oracle_event_lifecycle::<olivia_secp256k1::Secp256k1>(
            db.clone(),
        )
        .await;
    }

    #[tokio::test]
    async fn get_schema_version() {
        let docker = clients::Cli::default();
        let (url, _container) = new_backend!(docker);
        let db = PgBackendWrite::connect(&url).await.unwrap();
        db.setup().await.unwrap();
        let version = db.version().await.unwrap();
        assert_eq!(version.version, 0);
    }
}
