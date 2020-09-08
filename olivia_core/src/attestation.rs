use alloc::string::String;
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Attestation<C: crate::Curve> {
    pub outcome: String,
    pub time: chrono::NaiveDateTime,
    pub scalar: C::SchnorrScalar,
}

impl<C: crate::Curve> Attestation<C> {
    pub fn new(outcome: String, mut time: chrono::NaiveDateTime, scalar: C::SchnorrScalar) -> Self {
        use chrono::Timelike;
        time = time.with_nanosecond(0).expect("0 is valid");
        Attestation {
            outcome,
            time,
            scalar,
        }
    }
}
