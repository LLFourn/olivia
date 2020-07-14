use crate::curve::{ed25519, secp256k1};
use chrono::NaiveDateTime;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Attestation {
    pub outcome: String,
    pub time: NaiveDateTime,
    pub scalars: Scalars,
}

impl Attestation {
    pub fn new(outcome: String, mut time: NaiveDateTime, scalars: Scalars) -> Self {
        use chrono::Timelike;
        time = time.with_nanosecond(0).expect("0 is valid");
        Attestation {
            outcome,
            time,
            scalars,
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Scalars {
    pub ed25519: ed25519::SchnorrScalar,
    pub secp256k1: secp256k1::SchnorrScalar,
}
