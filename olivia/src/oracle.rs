use crate::{keychain::KeyChain, seed::Seed};
use anyhow::anyhow;
use olivia_core::{
    attest, AnnouncedEvent, Attestation, AttestationSchemes, Event, Group, OracleKeys,
    StampedOutcome,
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
    #[error("the announcement for this event was no longer valid when read from database")]
    AnnouncementWasBogus,
}

pub struct Oracle<C: Group> {
    db: Arc<dyn crate::db::Db<C>>,
    keychain: KeyChain<C>,
}

impl<C: Group> Oracle<C> {
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
        match self.db.get_announced_event(&new_event.id).await {
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
        let existing = self.db.get_announced_event(&stamped.outcome.id).await;
        let outcome_val_str = stamped.outcome.outcome_string();
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
            Ok(Some(AnnouncedEvent {
                event,
                announcement,
                ..
            })) => {
                if let Some(oracle_event) = announcement.verify_against_id(
                    &stamped.outcome.id,
                    &self.keychain.oracle_public_keys().announcement,
                ) {
                    let att_schemes =
                        AttestationSchemes {
                            olivia_v1: oracle_event.schemes.olivia_v1.as_ref().map(|_| {
                                attest::OliviaV1 {
                                    scalars: self
                                        .keychain
                                        .olivia_v1_scalars_for_event_outcome(&stamped),
                                }
                            }),
                            ecdsa_v1: oracle_event.schemes.ecdsa_v1.as_ref().map(|_| {
                                attest::EcdsaV1 {
                                    signature: self.keychain.ecdsa_sign_outcome(&stamped.outcome),
                                }
                            }),
                        };

                    let attestation = Attestation::new(outcome_val_str, stamped.time, att_schemes);

                    self.db
                        .complete_event(&event.id, attestation)
                        .await
                        .map_err(OutcomeResult::DbWriteErr)
                } else {
                    Err(OutcomeResult::AnnouncementWasBogus)
                }
            }
            Err(e) => Err(OutcomeResult::DbReadErr(e)),
        }
    }
}

#[cfg(test)]
pub mod test {
    use super::*;
    use crate::db::Db;
    use core::{convert::TryInto, str::FromStr};
    use olivia_core::{EventId, WireEventOutcome};

    pub async fn test_oracle_event_lifecycle<C: Group>(db: Arc<dyn Db<C>>) {
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
            .get_announced_event(&event_id)
            .await
            .unwrap()
            .expect("event should be there");

        let oracle_event = event
            .announcement
            .verify_against_id(&event_id, &public_keys.announcement)
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
            .get_announced_event(&event_id)
            .await
            .unwrap()
            .expect("event should still be there");

        let attestation = attested_event.attestation.expect("should be attested to");
        dbg!(&attestation, &oracle_event);
        assert_eq!(
            attestation.verify_olivia_v1_attestation(&oracle_event, &public_keys),
            Ok(())
        );
    }
}
