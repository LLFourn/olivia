use crate::{
    curve::{ed25519, secp256k1},
    db,
    event::{Attestation, Event, ObservedEvent, Outcome},
    keychain::KeyChain,
    seed::Seed,
};
use std::sync::Arc;

#[derive(PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct OraclePubkeys {
    pub ed25519: ed25519::PublicKey,
    pub secp256k1: secp256k1::PublicKey,
}

pub enum EventResult {
    AlreadyExists,
    AlreadyCompleted,
    Created,
    Changed,
    IncompatibleChange,
    DbReadErr(crate::db::Error),
    DbWriteErr(crate::db::Error),
}

pub enum OutcomeResult {
    Completed,
    AlreadyCompleted,
    OutcomeChanged { existing: String, new: String },
    EventNotExist,
    OutcomeNotExist { got: String, available: Vec<String> },
    DbReadErr(crate::db::Error),
    DbWriteErr(crate::db::Error),
}

impl EventResult {
    pub fn log(&self, logger: slog::Logger) {
        use EventResult::*;
        match self {
            Created => info!(logger, "created"),
            Changed => info!(logger, "changed"),
            AlreadyExists => debug!(logger, "ignored - already exists"),
            AlreadyCompleted => debug!(logger, "ignored - already completed"),
            IncompatibleChange => error!(logger, "incompatible change"),
            DbReadErr(e) => crit!(logger,"database read";"error" => format!("{}",e)),
            DbWriteErr(e) => crit!(logger,"database write"; "error" => format!("{}", e)),
        }
    }
}

impl OutcomeResult {
    pub fn log(&self, logger: slog::Logger) {
        use OutcomeResult::*;

        match self {
            Completed => info!(logger, "completed"),
            AlreadyCompleted => debug!(logger, "already completed"),
            OutcomeChanged { existing, new } => {
                crit!(logger, "outcome changed"; "existing" => existing, "new" => new)
            }
            EventNotExist => error!(logger, "event doesn't exist"),
            OutcomeNotExist { got, available } => {
                crit!(logger, "outcome not exist"; "got" => got, "available" => available.join(", "))
            }
            DbReadErr(e) => crit!(logger, "database read"; "error" => format!("{}", e)),
            DbWriteErr(e) => crit!(logger, "database write"; "error" => format!("{}", e)),
        }
    }
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

    pub async fn add_event(&self, new_event: Event) -> EventResult {
        match self.db.get_event(&new_event.id).await {
            Ok(Some(ObservedEvent {
                attestation: Some(_),
                ..
            })) => EventResult::AlreadyCompleted,
            Ok(Some(ObservedEvent { event, .. })) if event == new_event => {
                EventResult::AlreadyExists
            }
            Ok(Some(ObservedEvent { .. })) => unimplemented!("havent implemented updating yet"),
            Ok(None) => {
                let nonce = self.keychain.nonces_for_event(&new_event.id);
                let insert_result = self
                    .db
                    .insert_event(ObservedEvent {
                        event: new_event,
                        nonce,
                        attestation: None,
                    })
                    .await;

                match insert_result {
                    Ok(()) => EventResult::Created,
                    Err(e) => EventResult::DbWriteErr(e),
                }
            }
            Err(e) => EventResult::DbReadErr(e),
        }
    }

    pub async fn complete_event(&self, outcome: Outcome) -> OutcomeResult {
        let existing = self.db.get_event(&outcome.event_id).await;
        match existing {
            Ok(None) => OutcomeResult::EventNotExist,
            Ok(Some(ObservedEvent {
                attestation: Some(attestation),
                ..
            })) => {
                if attestation.outcome == outcome.outcome {
                    OutcomeResult::AlreadyCompleted
                } else {
                    OutcomeResult::OutcomeChanged {
                        existing: attestation.outcome,
                        new: outcome.outcome,
                    }
                }
            }
            Ok(Some(ObservedEvent { event, .. })) => {
                if !event
                    .outcomes()
                    .iter()
                    .any(|valid_outcome| valid_outcome.as_str() == outcome.outcome)
                {
                    return OutcomeResult::OutcomeNotExist {
                        got: outcome.outcome,
                        available: event.outcomes(),
                    };
                }

                let scalars = self
                    .keychain
                    .scalars_for_event_outcome(&event.id, &outcome.outcome);
                let attest = Attestation::new(outcome.outcome, outcome.time, scalars);

                match self.db.complete_event(&event.id, attest).await {
                    Ok(()) => OutcomeResult::Completed,
                    Err(e) => OutcomeResult::DbWriteErr(e),
                }
            }
            Err(e) => OutcomeResult::DbReadErr(e),
        }
    }
}
