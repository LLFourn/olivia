use crate::{
    db::*,
    event::{Attestation, Event, EventId, ObservedEvent, PathRef},
    oracle,
};
use async_trait::async_trait;
use std::{collections::HashMap, sync::RwLock};

#[derive(Default)]
pub struct InMemory {
    public_keys: RwLock<Option<oracle::OraclePubkeys>>,
    inner: RwLock<HashMap<EventId, ObservedEvent>>,
}

#[async_trait]
impl DbRead for InMemory {
    async fn get_event(&self, id: &EventId) -> Result<Option<ObservedEvent>, crate::db::Error> {
        let db = &*self.inner.read().unwrap();
        Ok(db.get(&id).map(Clone::clone))
    }

    async fn get_path(&self, path: PathRef<'_>) -> Result<Option<Item>, Error> {
        let path = path.as_str();
        let db = &*self.inner.read().unwrap();
        let mut children: Vec<String> = {
            let path = if path.is_empty() {
                "".to_string()
            } else {
                format!("{}/", &path)
            };

            db.keys()
                .into_iter()
                .filter_map(|key| {
                    let id = key.as_str();
                    if id.starts_with(&path) && id.len() > path.len() {
                        let end = id[path.len()..]
                            .find('/')
                            .map(|end| end + path.len())
                            .unwrap_or(id.len());
                        Some(id[..end].to_string())
                    } else {
                        None
                    }
                })
                .collect()
        };

        children.sort();
        children.dedup();

        let event = db.get(&EventId::from(path.to_string())).map(Clone::clone);

        if event.is_none() && children.len() == 0 {
            Ok(None)
        } else {
            Ok(Some(Item {
                event,
                children: children.into_iter().collect(),
            }))
        }
    }
}

#[async_trait]
impl DbWrite for InMemory {
    async fn insert_event(&self, observed_event: ObservedEvent) -> Result<(), crate::db::Error> {
        let db = &mut *self.inner.write().unwrap();
        db.insert(observed_event.event.id.clone(), observed_event);
        Ok(())
    }
    async fn complete_event(
        &self,
        event_id: &EventId,
        attestation: Attestation,
    ) -> Result<(), crate::db::Error> {
        let db = &mut *self.inner.write().unwrap();
        match db.get_mut(&event_id) {
            Some(ref mut event) => match event.attestation {
                Some(_) => Err("This event has already been attested to".to_string())?,
                ref mut slot => {
                    *slot = Some(attestation);
                    Ok(())
                }
            },
            None => Err("Cannot complete event that does not exist".to_string())?,
        }
    }
}

#[async_trait]
impl TimeTickerDb for InMemory {
    async fn latest_time_event(&self) -> Result<Option<Event>, crate::db::Error> {
        let db = self.inner.read().unwrap();
        let mut obs_events: Vec<&ObservedEvent> = db.values().collect();
        obs_events.sort_by_cached_key(|obs_event| obs_event.event.expected_outcome_time);
        Ok(obs_events.last().map(|obs_event| obs_event.event.clone()))
    }
    async fn earliest_unattested_time_event(&self) -> Result<Option<Event>, crate::db::Error> {
        let db = self.inner.read().unwrap();
        let mut obs_events: Vec<&ObservedEvent> = db
            .values()
            .filter(|obs_event| obs_event.attestation == None)
            .collect();
        obs_events.sort_by_cached_key(|obs_event| obs_event.event.expected_outcome_time);
        Ok(obs_events.first().map(|obs_event| obs_event.event.clone()))
    }
}

#[async_trait]
impl DbMeta for InMemory {
    async fn get_public_keys(&self) -> Result<Option<oracle::OraclePubkeys>, Error> {
        Ok(self.public_keys.read().unwrap().clone())
    }
    async fn set_public_keys(&self, public_keys: oracle::OraclePubkeys) -> Result<(), Error> {
        *self.public_keys.write().unwrap() = Some(public_keys);
        Ok(())
    }
}

impl Db for InMemory {}

#[cfg(test)]
mod test {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn generic_test_in_memory() {
        crate::db::test::test_db(Arc::new(InMemory::default()));
    }

    #[test]
    fn time_ticker_in_memory() {
        use crate::sources::time_ticker;
        let db = InMemory::default();

        let mut rt = tokio::runtime::Runtime::new().unwrap();

        for time_event in time_ticker::test::time_ticker_db_test_data() {
            rt.block_on(db.insert_event(time_event)).unwrap();
        }

        time_ticker::test::test_time_ticker_db(Arc::new(db));
    }
}
