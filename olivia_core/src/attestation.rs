use crate::{alloc::string::ToString, EventId, OracleEvent, Outcome};
use alloc::{string::String, vec::Vec};

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Attestation<C: crate::Schnorr> {
    pub outcome: String,
    pub scalars: Vec<C::SigScalar>,
    pub time: chrono::NaiveDateTime,
}

impl<C: crate::Schnorr> Attestation<C> {
    pub fn new(
        outcome: String,
        mut time: chrono::NaiveDateTime,
        scalars: Vec<C::SigScalar>,
    ) -> Self {
        use chrono::Timelike;
        time = time.with_nanosecond(0).expect("0 is valid");
        Attestation {
            outcome,
            scalars,
            time,
        }
    }

    pub fn verify_attestation(
        &self,
        oracle_event: &OracleEvent<C>,
        oracle_public_key: &C::PublicKey,
    ) -> bool {
        let outcome =
            match Outcome::try_from_id_and_outcome(oracle_event.event.id.clone(), &self.outcome) {
                Ok(outcome) => outcome,
                Err(_) => return false,
            };

        if self.scalars.len() != oracle_event.nonces.len() {
            return false;
        }

        let signatures = self
            .scalars
            .iter()
            .zip(oracle_event.nonces.iter())
            .map(|(scalar, nonce)| {
                C::signature_from_scalar_and_nonce(scalar.clone(), nonce.clone())
            })
            .collect::<Vec<_>>();

        signatures
            .iter()
            .zip(outcome.fragments())
            .all(|(signature, fragment)| {
                C::verify_signature(
                    oracle_public_key,
                    fragment.to_string().as_bytes(),
                    signature,
                )
            })
    }

    pub fn test_instance(event_id: &EventId) -> Self {
        let outcome = Outcome::test_instance(event_id);

        let fragments = outcome
            .fragments()
            .into_iter()
            .map(|fragment| {
                C::reveal_signature_s(
                    &C::test_keypair(),
                    C::test_nonce_keypair(),
                    fragment.to_string().as_bytes(),
                )
            })
            .collect();
        Attestation::new(
            outcome.value.to_string(),
            chrono::Utc::now().naive_utc(),
            fragments,
        )
    }
}
