use crate::db::*;
use anyhow::anyhow;
use async_trait::async_trait;
use olivia_core::{
    AnnouncedEvent, Attestation, Child, ChildDesc, Event, EventId, Group, OracleKeys, Path,
};
use std::{
    collections::HashMap,
    str::FromStr,
    sync::{Arc, RwLock},
};

#[derive(Clone)]
pub struct InMemory<C: Group> {
    public_keys: Arc<RwLock<Option<OracleKeys<C>>>>,
    inner: Arc<RwLock<HashMap<EventId, AnnouncedEvent<C>>>>,
    node_kinds: Arc<RwLock<HashMap<Path, NodeKind>>>,
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
impl<C: Group> DbReadOracle<C> for InMemory<C> {
    async fn get_announced_event(
        &self,
        id: &EventId,
    ) -> Result<Option<AnnouncedEvent<C>>, crate::db::Error> {
        let db = &*self.inner.read().unwrap();
        Ok(db.get(&id).map(Clone::clone))
    }

    async fn get_public_keys(&self) -> Result<Option<OracleKeys<C>>, Error> {
        Ok(self.public_keys.read().unwrap().as_ref().map(Clone::clone))
    }
}

#[async_trait]
impl<C: Group> DbReadEvent for InMemory<C> {
    async fn get_node(&self, node: PathRef<'_>) -> Result<Option<PathNode>, Error> {
        let db = &*self.inner.read().unwrap();
        let node_kinds = self.node_kinds.read().unwrap();
        let node_kind = node_kinds
            .get(&node.to_path())
            .cloned()
            .unwrap_or(NodeKind::List);

        let mut children_list: Vec<Child> = {
            let parent_prefix = if node == PathRef::root() {
                "/".to_string()
            } else {
                format!("{}/", node)
            };

            db.keys()
                .into_iter()
                .filter_map(|event_id| {
                    let child_path = event_id.path().as_str();
                    if let Some(remaining) = child_path.strip_prefix(&parent_prefix) {
                        let end = remaining.find('/').unwrap_or(remaining.len());
                        let name = remaining[..end].to_string();
                        let child_node = &child_path[..parent_prefix.len() + end];
                        let child = Child {
                            name,
                            kind: node_kinds
                                .get(&Path::from_str(child_node).unwrap())
                                .cloned()
                                .unwrap_or(NodeKind::List),
                        };

                        Some(child)
                    } else {
                        None
                    }
                })
                .collect()
        };

        children_list.sort_unstable_by_key(|child| child.name.clone());
        children_list.dedup();

        let child_desc = match node_kind {
            NodeKind::List => ChildDesc::List {
                list: children_list.clone(),
            },
            NodeKind::Range { range_kind } => match children_list.len() {
                0 => ChildDesc::List { list: vec![] },
                _ => ChildDesc::Range {
                    range_kind,
                    start: Some(children_list[0].clone()),
                    end: Some(children_list[children_list.len() - 1].clone()),
                },
            },
        };

        let events: Vec<EventId> = {
            db.keys()
                .into_iter()
                .filter(|key| {
                    if let Some(remaining) = key.as_str().strip_prefix(node.as_str()) {
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
            Ok(Some(PathNode { events, child_desc }))
        }
    }

    async fn latest_child_event(
        &self,
        path: PathRef<'_>,
        kind: EventKind,
    ) -> anyhow::Result<Option<Event>> {
        let db = self.inner.read().unwrap();
        let mut ann_events: Vec<&AnnouncedEvent<C>> = db
            .values()
            .filter(|ann_event| {
                ann_event.event.id.as_str().starts_with(path.as_str())
                    && ann_event.event.id.as_str().ends_with(&kind.to_string())
            })
            .collect();
        ann_events.sort_by_cached_key(|ann_event| ann_event.event.expected_outcome_time);
        Ok(ann_events.last().map(|ann_event| ann_event.event.clone()))
    }

    async fn earliest_unattested_child_event(
        &self,
        path: PathRef<'_>,
        kind: EventKind,
    ) -> anyhow::Result<Option<Event>> {
        let db = self.inner.read().unwrap();
        let mut ann_events: Vec<&AnnouncedEvent<C>> = db
            .values()
            .filter(|ann_event| {
                ann_event.event.id.as_str().starts_with(path.as_str())
                    && ann_event.event.id.as_str().ends_with(&kind.to_string())
                    && ann_event.attestation == None
            })
            .collect();
        ann_events.sort_by_cached_key(|ann_event| ann_event.event.expected_outcome_time);
        Ok(ann_events.first().map(|ann_event| ann_event.event.clone()))
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

    async fn insert_node(&self, node: Node) -> Result<(), Error> {
        let mut node_kinds = self.node_kinds.write().unwrap();
        node_kinds.insert(node.path, node.kind);
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
    event_db => event_db,
    curve => olivia_secp256k1::Secp256k1,
    {
        use std::sync::Arc;
        let db = InMemory::<olivia_secp256k1::Secp256k1>::default();
        let event_db: Arc<dyn DbReadEvent> = Arc::new(db.clone());
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

    #[tokio::test]
    async fn test_against_oracle() {
        let db = Arc::<InMemory<olivia_secp256k1::Secp256k1>>::default();
        crate::oracle::test::test_oracle_event_lifecycle(db).await
    }
}
