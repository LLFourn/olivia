use crate::{
    core::{AnnouncedEvent, Attestation, Event, EventOutcome, Schnorr},
    curve::DeriveKeyPair,
    db,
    keychain::KeyChain,
    seed::Seed,
};
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

pub struct Oracle<C: Schnorr + DeriveKeyPair> {
    db: Arc<dyn crate::db::Db<C>>,
    keychain: KeyChain<C>,
}

impl<C: Schnorr + DeriveKeyPair> Oracle<C> {
    pub async fn new(seed: Seed, db: Arc<dyn crate::db::Db<C>>) -> Result<Self, db::Error> {
        let keychain = KeyChain::new(seed);
        let public_key = keychain.oracle_public_key();
        if let Some(db_pubkey) = db.get_public_key().await? {
            if public_key != db_pubkey {
                return Err("public key derived from seed does not match database")?;
            }
        } else {
            db.set_public_key(public_key).await?
        }

        Ok(Self { db, keychain })
    }

    pub fn public_key(&self) -> C::PublicKey {
        self.keychain.oracle_public_key()
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
                let announcement = self.keychain.create_announcement(&new_event.id);
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

    pub async fn complete_event(&self, event_outcome: EventOutcome) -> Result<(), OutcomeResult> {
        let existing = self.db.get_event(&event_outcome.event_id).await;
        let outcome_str = format!("{}", event_outcome.outcome);
        match existing {
            Ok(None) => Err(OutcomeResult::EventNotExist),
            Ok(Some(AnnouncedEvent {
                attestation: Some(attestation),
                ..
            })) => {
                if attestation.outcome == outcome_str {
                    Err(OutcomeResult::AlreadyCompleted)
                } else {
                    Err(OutcomeResult::OutcomeChanged {
                        existing: attestation.outcome,
                        new: outcome_str,
                    })
                }
            }
            Ok(Some(AnnouncedEvent { event, .. })) => {
                let scalars = self.keychain.scalar_for_event_outcome(&event_outcome);
                let attest = Attestation::new(outcome_str, event_outcome.time, scalars);
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
        let public_key = db
            .get_public_key()
            .await
            .unwrap()
            .expect("creating oracle should have set public keys");
        let event_id = EventId::from_str("/foo/bar/baz?occur").unwrap();
        assert!(oracle.add_event(event_id.clone().into()).await.is_ok());

        let event = db
            .get_event(&event_id)
            .await
            .unwrap()
            .expect("event should be there");

        assert!(event.announcement.verify(&event_id, &public_key));

        let outcome: EventOutcome = WireEventOutcome {
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
        let signature = attested_event
            .attestation_signature()
            .expect("should be attested to");

        assert!(SchnorrImpl::verify_signature(
            &public_key,
            outcome.attestation_string().as_bytes(),
            &signature
        ));
    }
}
