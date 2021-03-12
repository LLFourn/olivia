use crate::{alloc::string::ToString, EventId, OracleEvent, Outcome};
use alloc::{string::String, vec::Vec};

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Attestation<C: crate::Group> {
    pub outcome: String,
    pub scalars: Vec<C::AttestScalar>,
    pub time: chrono::NaiveDateTime,
}

impl<C: crate::Group> Attestation<C> {
    pub fn new(
        outcome: String,
        mut time: chrono::NaiveDateTime,
        scalars: Vec<C::AttestScalar>,
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

        for (frag_index, index) in outcome.attestation_indexes().iter().enumerate() {
            if !C::verify_attest_scalar(oracle_public_key, &oracle_event.nonces[frag_index], *index as u32, &self.scalars[frag_index]) {
                return false;
            }
        }

        true
    }

    pub fn test_instance(event_id: &EventId) -> Self {
        let outcome = Outcome::test_instance(event_id);

        let nonces = (0..event_id.n_nonces()).map(|_|C::reveal_attest_scalar(
            &C::test_keypair(),
            C::test_nonce_keypair(),
            0,
        )).collect();

        Attestation::new(
            outcome.to_string(),
            chrono::Utc::now().naive_utc(),
            nonces,
        )
    }
}
