use crate::{
    event::{Attestation, Event, EventId, ObservedEvent},
    oracle,
};
pub mod diesel;
pub mod in_memory;
use async_trait::async_trait;

pub type Error = Box<dyn std::error::Error + Send + Sync>;

#[async_trait]
pub trait DbRead: Send + Sync {
    async fn get_event(&self, id: &EventId) -> Result<Option<ObservedEvent>, Error>;
}

#[async_trait]
pub trait DbWrite: Send + Sync {
    async fn insert_event(&self, observed_event: ObservedEvent) -> Result<(), Error>;
    async fn complete_event(&self, event_id: &EventId, outcome: Attestation) -> Result<(), Error>;
}

#[async_trait]
pub trait DbMeta: Send + Sync {
    async fn get_public_keys(&self) -> Result<Option<oracle::OraclePubkeys>, Error>;
    async fn set_public_keys(&self, public_keys: oracle::OraclePubkeys) -> Result<(), Error>;
}

#[async_trait]
pub trait TimeTickerDb {
    async fn latest_time_event(&self) -> Result<Option<Event>, Error>;
    async fn earliest_unattested_time_event(&self) -> Result<Option<Event>, Error>;
}

pub trait Db: DbRead + DbWrite + TimeTickerDb + DbMeta {}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        event::{Event, EventKind},
        keychain::KeyChain,
        seed::Seed,
    };
    use chrono::NaiveDateTime;
    use std::{str::FromStr, sync::Arc};

    const TEST_SEED: Seed = Seed::new([42u8; 64]);

    impl Attestation {
        pub fn test_new(event_id: &EventId, outcome: &str) -> Self {
            Attestation::new(
                outcome.into(),
                chrono::Utc::now().naive_utc(),
                KeyChain::new(TEST_SEED).scalars_for_event_outcome(event_id, outcome),
            )
        }
    }

    impl ObservedEvent {
        pub fn test_new(id: &EventId) -> Self {
            let time = NaiveDateTime::from_str("2015-09-18T23:56:04").unwrap();
            let event = Event {
                id: id.clone(),
                human_url: None,
                kind: EventKind::SingleOccurrence,
                expected_outcome_time: time,
            };
            ObservedEvent {
                event: event.clone(),
                nonce: KeyChain::new(TEST_SEED).nonces_for_event(&event.id),
                attestation: Some(Attestation::test_new(id, &event.outcomes()[0])),
            }
        }
    }

    impl From<Event> for ObservedEvent {
        fn from(event: Event) -> Self {
            let nonce = KeyChain::new(TEST_SEED).nonces_for_event(&event.id);
            ObservedEvent {
                event,
                nonce,
                attestation: None,
            }
        }
    }

    pub fn test_db(db: Arc<dyn Db>) {
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        {
            let event_id = EventId::from("/test/db/test_insert_unattested".to_string());
            let mut obs_event = ObservedEvent::test_new(&event_id);
            obs_event.attestation = None;

            rt.block_on(db.insert_event(obs_event.clone())).unwrap();
            let entry = rt.block_on(db.get_event(&event_id)).unwrap().unwrap();

            assert_eq!(
                entry, obs_event,
                "unattested entry retrieved should be same as inserted"
            );
        }

        {
            let event_id = EventId::from("/test/db/test_insert_attested".to_string());
            let obs_event = ObservedEvent::test_new(&event_id);
            rt.block_on(db.insert_event(obs_event.clone())).unwrap();
            let entry = rt.block_on(db.get_event(&event_id)).unwrap().unwrap();

            assert_eq!(
                entry, obs_event,
                "attested entry retrieved should be same as inserted"
            );
        }

        {
            let event_id =
                EventId::from("/test/db/test_insert_unattested_then_complete".to_string());
            let mut obs_event = ObservedEvent::test_new(&event_id);
            let attestation = obs_event.attestation.take().unwrap();

            rt.block_on(db.insert_event(obs_event.clone())).unwrap();
            rt.block_on(db.complete_event(&event_id, attestation.clone()))
                .unwrap();

            let entry = rt.block_on(db.get_event(&event_id)).unwrap().unwrap();

            obs_event.attestation = Some(attestation);
            assert_eq!(
                entry, obs_event,
                "event should have an attestation calling complete_event"
            );
        }

        {
            let event_id = EventId::from("/test/db/dont_exist".to_string());

            assert!(rt.block_on(db.get_event(&event_id)).unwrap().is_none())
        }
    }
}
