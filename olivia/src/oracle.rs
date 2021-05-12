use crate::{
    core::{AnnouncedEvent, Attestation, Event, Group, OracleKeys, StampedOutcome},
    curve::DeriveKeyPair,
    keychain::KeyChain,
    seed::Seed,
};
use anyhow::anyhow;
use std::sync::Arc;

#[derive(thiserror::Error, Debug)]
pub enum EventResult {
    #[error("event already exists")]
    AlreadyExists,
    #[error("event already exists and has been attested to")]
    AlreadyCompleted,
    #[error("event already exists but was updated")]
    Changed,
    #[error("unable to read from database: {0}")]
    DbReadErr(crate::db::Error),
    #[error("unable to write to database: {0}")]
    DbWriteErr(crate::db::Error),
}

#[derive(thiserror::Error, Debug)]
pub enum OutcomeResult {
    #[error("event already attested to")]
    AlreadyCompleted,
    #[error("event has already been attested to with '{existing}' but you are trying to set it to '{new}'")]
    OutcomeChanged { existing: String, new: String },
    #[error("the event being attested to does not exist")]
    EventNotExist,
    #[error("unable to read from database: {0}")]
    DbReadErr(crate::db::Error),
    #[error("unable to write to database: {0}")]
    DbWriteErr(crate::db::Error),
}

pub struct Oracle<C: Group + DeriveKeyPair> {
    db: Arc<dyn crate::db::Db<C>>,
    keychain: KeyChain<C>,
}

impl<C: Group + DeriveKeyPair> Oracle<C> {
    pub async fn new(seed: Seed, db: Arc<dyn crate::db::Db<C>>) -> anyhow::Result<Self> {
        let keychain = KeyChain::new(seed);
        let public_keys = keychain.oracle_public_keys();
        if let Some(db_pubkeys) = db.get_public_keys().await? {
            if public_keys != db_pubkeys {
                return Err(anyhow!(
                    "public key derived from seed does not match database"
                ));
            }
        } else {
            db.set_public_keys(public_keys).await?
        }

        Ok(Self { db, keychain })
    }

    pub fn public_keys(&self) -> OracleKeys<C> {
        self.keychain.oracle_public_keys()
    }

    pub async fn add_event(&self, new_event: Event) -> Result<(), EventResult> {
        match self.db.get_event(&new_event.id).await {
            Ok(Some(AnnouncedEvent {
                attestation: Some(_),
                ..
            })) => Err(EventResult::AlreadyCompleted),
            Ok(Some(AnnouncedEvent { .. })) => {
                // TODO: update exected_outcome_time
                Err(EventResult::AlreadyExists)
            }
            Ok(None) => {
                let announcement = self.keychain.create_announcement(new_event.clone());
                self.db
                    .insert_event(AnnouncedEvent {
                        event: new_event,
                        announcement,
                        attestation: None,
                    })
                    .await
                    .map_err(EventResult::DbWriteErr)
            }
            Err(e) => Err(EventResult::DbReadErr(e)),
        }
    }

    pub async fn complete_event(&self, stamped: StampedOutcome) -> Result<(), OutcomeResult> {
        let existing = self.db.get_event(&stamped.outcome.id).await;
        let outcome_val_str = stamped.outcome.outcome_str();
        match existing {
            Ok(None) => Err(OutcomeResult::EventNotExist),
            Ok(Some(AnnouncedEvent {
                attestation: Some(attestation),
                ..
            })) => {
                if attestation.outcome == outcome_val_str {
                    Err(OutcomeResult::AlreadyCompleted)
                } else {
                    Err(OutcomeResult::OutcomeChanged {
                        existing: attestation.outcome,
                        new: outcome_val_str,
                    })
                }
            }
            Ok(Some(AnnouncedEvent { event, .. })) => {
                let scalars = self.keychain.scalars_for_event_outcome(&stamped);
                let attest = Attestation::new(outcome_val_str, stamped.time, scalars);
                self.db
                    .complete_event(&event.id, attest)
                    .await
                    .map_err(OutcomeResult::DbWriteErr)
            }
            Err(e) => Err(OutcomeResult::DbReadErr(e)),
        }
    }
}

#[cfg(test)]
pub mod test {
    use super::*;
    use crate::{
        core::{EventId, WireEventOutcome},
        curve::SchnorrImpl,
        db::Db,
    };
    use core::{convert::TryInto, str::FromStr};

    pub async fn test_oracle_event_lifecycle(db: Arc<dyn Db<SchnorrImpl>>) {
        let oracle = Oracle::new(crate::seed::Seed::new([42u8; 64]), db.clone())
            .await
            .expect("should be able to create oracle");
        let public_keys = db
            .get_public_keys()
            .await
            .unwrap()
            .expect("creating oracle should have set public keys");
        let event_id = EventId::from_str("/foo/bar/baz.occur").unwrap();
        assert!(oracle.add_event(event_id.clone().into()).await.is_ok());

        let event = db
            .get_event(&event_id)
            .await
            .unwrap()
            .expect("event should be there");

        let oracle_event = event
            .announcement
            .verify_against_id(&event_id, &public_keys.announcement_key)
            .expect("announcement signature should be valid");

        let outcome: StampedOutcome = WireEventOutcome {
            event_id: event_id.clone(),
            outcome: "true".into(),
            time: None,
        }
        .try_into()
        .unwrap();

        assert!(oracle.complete_event(outcome.clone()).await.is_ok());

        let attested_event = db
            .get_event(&event_id)
            .await
            .unwrap()
            .expect("event should still be there");

        let attestation = attested_event.attestation.expect("should be attested to");
        assert!(attestation.verify_attestation(&oracle_event, &public_keys.attestation_key));
    }
}
