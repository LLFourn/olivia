use crate::{alloc::string::ToString, EventId, Group, OracleEvent, OracleKeys, Outcome};
use alloc::{string::String, vec::Vec};

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(bound = "C: Group")]
pub struct Attestation<C: Group> {
    pub outcome: String,
    pub schemes: AttestationSchemes<C>,
    pub time: chrono::NaiveDateTime,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
#[serde(bound = "C: Group")]
pub struct AttestationSchemes<C: Group> {
    pub olivia_v1: Option<attest::OliviaV1<C>>,
    pub ecdsa_v1: Option<attest::EcdsaV1<C>>,
}

pub mod attest {
    use super::*;
    #[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
    pub struct OliviaV1<C: Group> {
        pub scalars: Vec<C::AttestScalar>,
    }

    #[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
    pub struct EcdsaV1<C: Group> {
        pub signature: C::EcdsaSignature,
    }
}

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum AttestationInvalid {
    #[error("olivia-v1 attestation was invalid")]
    OliviaV1,
    #[error("ecdsa-v1 attestation was invalid")]
    EcdsaV1,
    #[error("outcome is invalid")]
    Outcome,
    #[error("outcome is missing")]
    Missing,
}

impl<C: crate::Group> Attestation<C> {
    pub fn new(
        outcome: String,
        mut time: chrono::NaiveDateTime,
        schemes: AttestationSchemes<C>,
    ) -> Self {
        use chrono::Timelike;
        time = time.with_nanosecond(0).expect("0 is valid");
        Attestation {
            outcome,
            schemes,
            time,
        }
    }

    pub fn verify_attestation(
        &self,
        oracle_event: &OracleEvent<C>,
        oracle_keys: &OracleKeys<C>,
    ) -> Result<(), AttestationInvalid> {
        let outcome =
            match Outcome::try_from_id_and_outcome(oracle_event.event.id.clone(), &self.outcome) {
                Ok(outcome) => outcome,
                Err(_) => return Err(AttestationInvalid::Outcome),
            };

        match (&oracle_event.schemes.olivia_v1, &self.schemes.olivia_v1) {
            (Some(ann_olivia_v1), Some(att_olivia_v1)) => {
                if ann_olivia_v1.nonces.len() != att_olivia_v1.scalars.len() {
                    return Err(AttestationInvalid::OliviaV1);
                }

                for (frag_index, index) in outcome.attestation_indexes().iter().enumerate() {
                    if !C::verify_attest_scalar(
                        &oracle_keys.attestation_key,
                        &ann_olivia_v1.nonces[frag_index],
                        *index as u32,
                        &att_olivia_v1.scalars[frag_index],
                    ) {
                        return Err(AttestationInvalid::OliviaV1);
                    }
                }
            }
            (Some(_), None) => return Err(AttestationInvalid::Missing),
            _ => {}
        }

        match (&oracle_event.schemes.ecdsa_v1, &self.schemes.ecdsa_v1) {
            (Some(_), Some(attest::EcdsaV1 { signature })) => {
                if !C::ecdsa_verify(
                    &oracle_keys.announcement_key,
                    outcome.attestation_string().as_ref(),
                    signature,
                ) {
                    return Err(AttestationInvalid::EcdsaV1);
                }
            }
            (Some(_), None) => return Err(AttestationInvalid::Missing),
            _ => {}
        }

        Ok(())
    }

    pub fn test_instance(event_id: &EventId) -> Self {
        let outcome = Outcome::test_instance(event_id);

        let scalars = (0..event_id.n_nonces())
            .map(|_| C::reveal_attest_scalar(&C::test_keypair(), C::test_nonce_keypair(), 0))
            .collect();

        let schemes = AttestationSchemes {
            olivia_v1: Some(attest::OliviaV1 { scalars }),
            ecdsa_v1: Some(attest::EcdsaV1 {
                signature: C::ecdsa_sign(&C::test_keypair(), &outcome.to_string().as_bytes()),
            }),
        };

        Attestation::new(outcome.to_string(), chrono::Utc::now().naive_utc(), schemes)
    }
}
