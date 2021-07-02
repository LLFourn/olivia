use crate::db::*;
use anyhow::anyhow;
use async_trait::async_trait;
use olivia_core::{AnnouncedEvent, Attestation, ChildDesc, Event, EventId, Group, OracleKeys};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

#[derive(Clone)]
pub struct InMemory<C: Group> {
    public_keys: Arc<RwLock<Option<OracleKeys<C>>>>,
    inner: Arc<RwLock<HashMap<EventId, AnnouncedEvent<C>>>>,
    node_kinds: Arc<RwLock<HashMap<String, ChildrenKind>>>,
}

impl<C: Group> Default for InMemory<C> {
    fn default() -> Self {
        Self {
            public_keys: Arc::new(RwLock::new(None)),
            inner: Arc::new(RwLock::new(HashMap::default())),
            node_kinds: Arc::new(RwLock::new(HashMap::default())),
        }
    }
}

#[async_trait]
impl<C: Group> DbRead<C> for InMemory<C> {
    async fn get_event(&self, id: &EventId) -> Result<Option<AnnouncedEvent<C>>, crate::db::Error> {
        let db = &*self.inner.read().unwrap();
        Ok(db.get(&id).map(Clone::clone))
    }

    async fn get_node(&self, node: &str) -> Result<Option<DbNode>, Error> {
        let db = &*self.inner.read().unwrap();
        let node_kind = {
            let node_kinds = self.node_kinds.read().unwrap();
            match node_kinds.get(node).cloned() {
                Some(node_kind) => node_kind,
                None => ChildrenKind::List,
            }
        };

        let mut children_list: Vec<String> = {
            let path = if node == "/" {
                "/".to_string()
            } else {
                format!("{}/", node)
            };

            db.keys()
                .into_iter()
                .filter_map(|key| {
                    let key = key.as_str();
                    if let Some(remaining) = key.strip_prefix(&path) {
                        let end = remaining
                            .find(['/', '.'].as_ref())
                            .expect("always has a ‘.’");

                        Some(remaining[..end].to_string())
                    } else {
                        None
                    }
                })
                .collect()
        };

        children_list.sort();
        children_list.dedup();

        let child_desc = match node_kind {
            ChildrenKind::List => ChildDesc::List {
                list: children_list.clone(),
            },
            ChildrenKind::Range { range_kind } => match children_list.len() {
                0 => ChildDesc::List { list: vec![] },
                _ => ChildDesc::Range {
                    range_kind,
                    start: children_list[0].clone(),
                    end: children_list[children_list.len() - 1].clone(),
                },
            },
        };

        let events: Vec<EventId> = {
            db.keys()
                .into_iter()
                .filter(|key| {
                    if let Some(remaining) = key.as_str().strip_prefix(node) {
                        remaining.starts_with('.')
                    } else {
                        false
                    }
                })
                .map(Clone::clone)
                .collect()
        };

        if events.is_empty() && children_list.is_empty() {
            Ok(None)
        } else {
            Ok(Some(DbNode { events, child_desc }))
        }
    }

    async fn latest_child_event(
        &self,
        path: &str,
        kind: EventKind,
    ) -> anyhow::Result<Option<Event>> {
        let db = self.inner.read().unwrap();
        let mut obs_events: Vec<&AnnouncedEvent<C>> = db
            .values()
            .filter(|obs_event| {
                obs_event.event.id.as_str().starts_with(path)
                    && obs_event.event.id.as_str().ends_with(&kind.to_string())
            })
            .collect();
        obs_events.sort_by_cached_key(|obs_event| obs_event.event.expected_outcome_time);
        Ok(obs_events.last().map(|obs_event| obs_event.event.clone()))
    }

    async fn earliest_unattested_child_event(
        &self,
        path: &str,
        kind: EventKind,
    ) -> anyhow::Result<Option<Event>> {
        let db = self.inner.read().unwrap();
        let mut obs_events: Vec<&AnnouncedEvent<C>> = db
            .values()
            .filter(|obs_event| {
                obs_event.event.id.as_str().starts_with(path)
                    && obs_event.event.id.as_str().ends_with(&kind.to_string())
                    && obs_event.attestation == None
            })
            .collect();
        obs_events.sort_by_cached_key(|obs_event| obs_event.event.expected_outcome_time);
        Ok(obs_events.first().map(|obs_event| obs_event.event.clone()))
    }

    async fn get_public_keys(&self) -> Result<Option<OracleKeys<C>>, Error> {
        Ok(self.public_keys.read().unwrap().as_ref().map(Clone::clone))
    }
}

#[async_trait]
impl<C: Group> DbWrite<C> for InMemory<C> {
    async fn insert_event(
        &self,
        observed_event: AnnouncedEvent<C>,
    ) -> Result<(), crate::db::Error> {
        let db = &mut *self.inner.write().unwrap();
        db.insert(observed_event.event.id.clone(), observed_event);
        Ok(())
    }
    async fn complete_event(
        &self,
        event_id: &EventId,
        attestation: Attestation<C>,
    ) -> Result<(), crate::db::Error> {
        let db = &mut *self.inner.write().unwrap();
        match db.get_mut(&event_id) {
            Some(ref mut event) => match event.attestation {
                Some(_) => Err(anyhow!("This event has already been attested to")),
                ref mut slot => {
                    *slot = Some(attestation);
                    Ok(())
                }
            },
            None => Err(anyhow!("Cannot complete event that does not exist")),
        }
    }

    async fn set_public_keys(&self, public_keys: OracleKeys<C>) -> Result<(), Error> {
        *self.public_keys.write().unwrap() = Some(public_keys);
        Ok(())
    }

    async fn set_node_kind(&self, path: &str, kind: ChildrenKind) -> Result<(), Error> {
        let mut node_kinds = self.node_kinds.write().unwrap();
        node_kinds.insert(path.into(), kind);
        Ok(())
    }
}

impl<C: Group> Db<C> for InMemory<C> {}

impl<C: Group> BorrowDb<C> for InMemory<C> {
    fn borrow_db(&self) -> &dyn Db<C> {
        self
    }
}

#[cfg(test)]
crate::run_rest_api_tests! {
    oracle => oracle,
    routes => routes,
    curve => olivia_secp256k1::Secp256k1,
    {
        let db = InMemory::<olivia_secp256k1::Secp256k1>::default();
        let oracle = crate::oracle::Oracle::new(crate::seed::Seed::new([42u8; 64]), Arc::new(db.clone())).await.unwrap();
        let routes = crate::rest_api::routes(Arc::new(db), slog::Logger::root(slog::Discard, o!()));
    }
}

#[cfg(test)]
crate::run_time_db_tests! {
    db => db,
    curve => olivia_secp256k1::Secp256k1,
    {
        use std::sync::Arc;
        let db = InMemory::<olivia_secp256k1::Secp256k1>::default();
        let db: Arc<dyn Db<olivia_secp256k1::Secp256k1>> = Arc::new(db);
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn generic_test_in_memory() {
        let db = InMemory::<olivia_secp256k1::Secp256k1>::default();
        crate::db::test::test_db(&db).await;
    }

    // #[tokio::test]
    // async fn time_ticker_in_memory() {
    //     use crate::sources::time_ticker;
    //     time_ticker::test::run_time_db_tests(async || InMemory::<SchnorrImpl>::default()).await;
    // }

    #[tokio::test]
    async fn test_against_oracle() {
        let db = Arc::<InMemory<olivia_secp256k1::Secp256k1>>::default();
        crate::oracle::test::test_oracle_event_lifecycle(db).await
    }
}
