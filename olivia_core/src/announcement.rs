use crate::{Attestation, Descriptor, Event, EventId, Group};
use chrono::NaiveDateTime;
use core::{convert::TryFrom, marker::PhantomData};

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(bound = "C: Group")]
pub struct RawAnnouncement<C: Group> {
    pub oracle_event: RawOracleEvent<C>,
    pub signature: C::Signature,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RawOracleEvent<C> {
    #[serde(flatten)]
    payload: RawOracleEventEncoding,
    #[serde(skip_serializing, default)]
    curve: PhantomData<C>,
}

impl<C: Group> RawOracleEvent<C> {
    #[must_use]
    pub fn verify(
        &self,
        oracle_public_key: &C::PublicKey,
        announcement_signature: &C::Signature,
    ) -> bool {
        C::verify_announcement_signature(
            oracle_public_key,
            self.payload.as_bytes(),
            announcement_signature,
        )
    }

    pub fn decode(&self) -> Option<OracleEvent<C>> {
        self.payload.decode()
    }

    pub fn sign(&self, keypair: &C::KeyPair) -> C::Signature {
        C::sign_announcement(keypair, self.payload.as_bytes())
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.payload.as_bytes()
    }

    pub fn from_json_bytes(bytes: Vec<u8>) -> Self {
        Self {
            payload: RawOracleEventEncoding::Json(String::from_utf8(bytes).expect("valid JSON")),
            curve: PhantomData,
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case", tag = "encoding", content = "data")]
enum RawOracleEventEncoding {
    Json(String),
}

impl RawOracleEventEncoding {
    fn decode<'a, C: Group>(&'a self) -> Option<OracleEvent<C>> {
        use RawOracleEventEncoding::*;
        match self {
            Json(string) => serde_json::from_str(string).ok(),
        }
    }

    fn as_bytes(&self) -> &[u8] {
        use RawOracleEventEncoding::*;
        match self {
            Json(string) => string.as_bytes(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
#[serde(bound = "C: Group")]
pub struct OracleEventWithDescriptor<C: Group> {
    pub id: EventId,
    pub expected_outcome_time: Option<NaiveDateTime>,
    pub descriptor: Descriptor,
    pub schemes: AnnouncementSchemes<C>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(bound = "C: Group")]
pub struct AnnouncementSchemes<C: Group> {
    pub olivia_v1: Option<announce::OliviaV1<C>>,
    pub ecdsa_v1: Option<announce::EcdsaV1>,
}

pub mod announce {
    use super::*;
    #[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
    #[serde(rename_all = "kebab-case")]
    pub struct OliviaV1<C: Group> {
        pub nonces: Vec<C::PublicNonce>,
    }

    #[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
    #[serde(rename_all = "kebab-case")]
    pub struct EcdsaV1 {}
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(
    try_from = "OracleEventWithDescriptor<C>",
    into = "OracleEventWithDescriptor<C>",
    bound = "C: Group"
)]
pub struct OracleEvent<C: Group> {
    pub event: Event,
    pub schemes: AnnouncementSchemes<C>,
}

impl<C: Group> TryFrom<OracleEventWithDescriptor<C>> for OracleEvent<C> {
    type Error = String;

    fn try_from(oracle_event: OracleEventWithDescriptor<C>) -> Result<Self, Self::Error> {
        let schemes = &oracle_event.schemes;

        if let Some(olivia_v1) = &schemes.olivia_v1 {
            if olivia_v1.nonces.len() < oracle_event.id.n_nonces() as usize {
                return Err("oracle event doesn't have enough nonces for descriptor".into());
            }
        }

        if oracle_event.id.descriptor() == oracle_event.descriptor {
            Ok(OracleEvent {
                event: Event {
                    id: oracle_event.id,
                    expected_outcome_time: oracle_event.expected_outcome_time,
                },
                schemes: oracle_event.schemes,
            })
        } else {
            Err("descriptor doesn't match event id".into())
        }
    }
}

impl<C: Group> From<OracleEvent<C>> for OracleEventWithDescriptor<C> {
    fn from(oracle_event: OracleEvent<C>) -> Self {
        let descriptor = oracle_event.event.id.descriptor();
        OracleEventWithDescriptor {
            id: oracle_event.event.id,
            expected_outcome_time: oracle_event.event.expected_outcome_time,
            descriptor,
            schemes: oracle_event.schemes,
        }
    }
}

impl<C: Group> OracleEvent<C> {
    fn encode_json(&self) -> RawOracleEvent<C> {
        RawOracleEvent {
            payload: RawOracleEventEncoding::Json(serde_json::to_string(self).unwrap()),
            curve: PhantomData,
        }
    }

    pub fn anticipate_attestations_olivia_v1(
        &self,
        public_key: &C::PublicKey,
        nonce_index: usize,
    ) -> Option<Vec<C::AnticipatedAttestation>> {
        self.schemes.olivia_v1.as_ref().map(|olivia_v1| {
            C::anticipate_attestations(
                public_key,
                &olivia_v1.nonces[nonce_index],
                self.event.id.n_outcomes_for_nonce(nonce_index),
            )
        })
    }
}

impl<C: Group> RawAnnouncement<C> {
    #[must_use]
    pub fn verify_against_id(
        &self,
        event_id: &EventId,
        oracle_announcement_key: &C::PublicKey,
    ) -> Option<OracleEvent<C>> {
        if !self
            .oracle_event
            .verify(oracle_announcement_key, &self.signature)
        {
            return None;
        }

        let oracle_event = self.oracle_event.decode()?;

        if oracle_event.event.id != *event_id {
            return None;
        }

        Some(oracle_event)
    }

    pub fn create(event: Event, keypair: &C::KeyPair, schemes: AnnouncementSchemes<C>) -> Self {
        let oracle_event = OracleEvent::<C> { event, schemes };

        let encoded_oracle_event = oracle_event.encode_json();
        let signature = encoded_oracle_event.sign(keypair);
        Self {
            signature,
            oracle_event: encoded_oracle_event,
        }
    }

    pub fn test_instance(event: Event) -> Self {
        let nonces: Vec<_> = (0..event.id.event_kind().n_nonces())
            .map(|_| C::test_nonce_keypair().into())
            .collect();
        Self::create(
            event.clone(),
            &C::test_keypair(),
            AnnouncementSchemes {
                olivia_v1: match nonces.is_empty() {
                    true => None,
                    false => Some(announce::OliviaV1 { nonces }),
                },
                ecdsa_v1: Some(announce::EcdsaV1 {}),
            },
        )
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(bound = "C: Group")]
pub struct AnnouncedEvent<C: Group> {
    pub event: Event,
    pub announcement: RawAnnouncement<C>,
    pub attestation: Option<Attestation<C>>,
}

impl<C: Group> AnnouncedEvent<C> {
    pub fn test_attested_instance(event: Event) -> Self {
        Self {
            event: event.clone(),
            announcement: RawAnnouncement::test_instance(event.clone()),
            attestation: Some(Attestation::test_instance(&event.id)),
        }
    }

    pub fn test_unattested_instance(event: Event) -> Self {
        let mut unattested = Self::test_attested_instance(event);
        unattested.attestation = None;
        unattested
    }
}
