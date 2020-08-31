use crate::{
    core::{AnnouncedEvent, Attestation, Event, EventOutcome},
    curve::{
        ed25519::{self},
        secp256k1::{self},
    },
    db,
    keychain::KeyChain,
    seed::Seed,
};
use std::sync::Arc;

#[derive(PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct OraclePubkeys {
    pub ed25519: ed25519::PublicKey,
    pub secp256k1: secp256k1::PublicKey,
}

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

pub struct Oracle {
    db: Arc<dyn crate::db::Db>,
    keychain: KeyChain,
}

impl Oracle {
    pub async fn new(seed: Seed, db: Arc<dyn crate::db::Db>) -> Result<Self, db::Error> {
        let keychain = KeyChain::new(seed);
        let public_keys = keychain.oracle_pubkeys();
        if let Some(db_pubkeys) = db.get_public_keys().await? {
            if public_keys != db_pubkeys {
                return Err("public keys derived from seed do not match those in database")?;
            }
        } else {
            db.set_public_keys(public_keys).await?
        }

        Ok(Self { db, keychain })
    }

    pub fn public_keys(&self) -> OraclePubkeys {
        self.keychain.oracle_pubkeys()
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
                let scalars = self.keychain.scalars_for_event_outcome(&event_outcome);
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
        curve::{ed25519::Ed25519, secp256k1::Secp256k1, Curve},
        db::Db,
    };
    use core::{convert::TryInto, str::FromStr};

    pub async fn test_oracle_event_lifecycle(db: Arc<dyn Db>) {
        let oracle = Oracle::new(crate::seed::Seed::new([42u8; 64]), db.clone())
            .await
            .expect("should be able to create oracle");
        let public_keys = db
            .get_public_keys()
            .await
            .unwrap()
            .expect("creating oracle should have set public keys");
        let event_id = EventId::from_str("/foo/bar/baz?occur").unwrap();
        assert!(
            oracle.add_event(event_id.clone().into()).await.is_ok()
        );

        db.get_event(&event_id)
            .await
            .unwrap()
            .expect("event should be there");

        let outcome: EventOutcome = WireEventOutcome {
            event_id: event_id.clone(),
            outcome: "true".into(),
            time: None,
        }
        .try_into()
        .unwrap();

        assert!(
             oracle.complete_event(outcome.clone()).await.is_ok()
        );

        let obs_event = db
            .get_event(&event_id)
            .await
            .unwrap()
            .expect("event should still be there");
        let signatures = obs_event.signatures().expect("should be attested to");

        assert!(Secp256k1::verify_signature(
            &public_keys.secp256k1,
            outcome.attestation_string().as_bytes(),
            &signatures.secp256k1
        ));

        assert!(Ed25519::verify_signature(
            &public_keys.ed25519,
            outcome.attestation_string().as_bytes(),
            &signatures.ed25519
        ));
    }
}
