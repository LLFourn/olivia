use crate::{
    core::{AnnouncedEvent, Attestation, Event, EventId, Group},
    db::*,
};
use anyhow::anyhow;
use async_trait::async_trait;
use std::{collections::HashMap, sync::RwLock};

pub struct InMemory<C: Group> {
    public_key: RwLock<Option<C::PublicKey>>,
    inner: RwLock<HashMap<EventId, AnnouncedEvent<C>>>,
}

impl<C: Group> Default for InMemory<C> {
    fn default() -> Self {
        Self {
            public_key: RwLock::new(None),
            inner: RwLock::new(HashMap::default()),
        }
    }
}

#[async_trait]
impl<C: Group> DbRead<C> for InMemory<C> {
    async fn get_event(&self, id: &EventId) -> Result<Option<AnnouncedEvent<C>>, crate::db::Error> {
        let db = &*self.inner.read().unwrap();
        Ok(db.get(&id).map(Clone::clone))
    }

    async fn get_node(&self, node: &str) -> Result<Option<Item>, Error> {
        let db = &*self.inner.read().unwrap();
        let mut children: Vec<String> = {
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
                            .find(['/', '?'].as_ref())
                            .map(|end| end + path.len())
                            .expect("always has a ‘?’");

                        Some(key[..end].to_string())
                    } else {
                        None
                    }
                })
                .collect()
        };

        children.sort();
        children.dedup();

        let events: Vec<EventId> = {
            db.keys()
                .into_iter()
                .filter(|key| {
                    if let Some(remaining) = key.as_str().strip_prefix(node) {
                        remaining.starts_with('?')
                    } else {
                        false
                    }
                })
                .map(Clone::clone)
                .collect()
        };

        if events.is_empty() && children.is_empty() {
            Ok(None)
        } else {
            Ok(Some(Item { events, children }))
        }
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
}

#[async_trait]
impl<C: Group> TimeTickerDb for InMemory<C> {
    async fn latest_time_event(&self) -> Result<Option<Event>, crate::db::Error> {
        let db = self.inner.read().unwrap();
        let mut obs_events: Vec<&AnnouncedEvent<C>> = db
            .values()
            .filter(|obs_event| obs_event.event.id.as_str().starts_with("/time"))
            .collect();
        obs_events.sort_by_cached_key(|obs_event| obs_event.event.expected_outcome_time);
        Ok(obs_events.last().map(|obs_event| obs_event.event.clone()))
    }
    async fn earliest_unattested_time_event(&self) -> Result<Option<Event>, crate::db::Error> {
        let db = self.inner.read().unwrap();
        let mut obs_events: Vec<&AnnouncedEvent<C>> = db
            .values()
            .filter(|obs_event| {
                obs_event.event.id.as_str().starts_with("/time") && obs_event.attestation == None
            })
            .collect();
        obs_events.sort_by_cached_key(|obs_event| obs_event.event.expected_outcome_time);
        Ok(obs_events.first().map(|obs_event| obs_event.event.clone()))
    }
}

#[async_trait]
impl<C: Group> DbMeta<C> for InMemory<C> {
    async fn get_public_key(&self) -> Result<Option<C::PublicKey>, Error> {
        Ok(self.public_key.read().unwrap().as_ref().map(Clone::clone))
    }

    async fn set_public_key(&self, public_key: C::PublicKey) -> Result<(), Error> {
        *self.public_key.write().unwrap() = Some(public_key);
        Ok(())
    }
}

impl<C: Group> Db<C> for InMemory<C> {}

#[cfg(test)]
mod test {
    use super::*;
    use crate::curve::SchnorrImpl;
    use std::sync::Arc;

    #[test]
    fn generic_test_in_memory() {
        let db = Arc::new(InMemory::<SchnorrImpl>::default());
        crate::db::test::test_db(db.as_ref());
    }

    #[tokio::test]
    async fn time_ticker_in_memory() {
        use crate::sources::time_ticker;
        let db = InMemory::<SchnorrImpl>::default();

        for time_event in time_ticker::test::time_ticker_db_test_data() {
            db.insert_event(time_event).await.unwrap();
        }

        time_ticker::test::test_time_ticker_db(Arc::new(db)).await;
    }

    #[tokio::test]
    async fn test_against_oracle() {
        let db = InMemory::<SchnorrImpl>::default();
        crate::oracle::test::test_oracle_event_lifecycle(Arc::new(db)).await
    }
}
