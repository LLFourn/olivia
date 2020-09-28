use crate::{EventId, EventOutcome};
use alloc::string::String;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Attestation<C: crate::Schnorr> {
    pub outcome: String,
    pub time: chrono::NaiveDateTime,
    pub scalar: C::SigScalar,
}


impl<C: crate::Schnorr> Attestation<C> {
    pub fn new(outcome: String, mut time: chrono::NaiveDateTime, scalar: C::SigScalar) -> Self {
        use chrono::Timelike;
        time = time.with_nanosecond(0).expect("0 is valid");
        Attestation {
            outcome,
            time,
            scalar,
        }
    }

    pub fn test_instance(event_id: &EventId) -> Self {
        let event_outcome = EventOutcome::test_instance(event_id);
        Attestation::new(
            format!("{}", event_outcome.outcome),
            chrono::Utc::now().naive_utc(),
            C::reveal_signature_s(
                &C::test_keypair(),
                C::test_nonce_keypair(),
                event_outcome.attestation_string().as_bytes(),
            ),
        )
    }
}
