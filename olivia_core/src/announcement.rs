use crate::{Attestation, Descriptor, Event, EventId, Group};
use alloc::{string::String, vec::Vec};
use chrono::NaiveDateTime;
use core::{convert::TryFrom, marker::PhantomData};

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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
    fn decode<C: Group>(&self) -> Option<OracleEvent<C>> {
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
pub struct OracleEventWithDescriptor<C: Group> {
    pub id: EventId,
    pub expected_outcome_time: Option<NaiveDateTime>,
    pub descriptor: Descriptor,
    pub nonces: Vec<C::PublicNonce>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(
    try_from = "OracleEventWithDescriptor<C>",
    into = "OracleEventWithDescriptor<C>"
)]
pub struct OracleEvent<C: Group> {
    pub event: Event,
    pub nonces: Vec<C::PublicNonce>,
}

impl<C: Group> TryFrom<OracleEventWithDescriptor<C>> for OracleEvent<C> {
    type Error = String;

    fn try_from(oracle_event: OracleEventWithDescriptor<C>) -> Result<Self, Self::Error> {
        if oracle_event.nonces.len() < oracle_event.descriptor.n_nonces() {
            return Err("oracle event doesn't have enough nonces for descriptor".into());
        }

        if oracle_event.id.descriptor() == oracle_event.descriptor {
            Ok(OracleEvent {
                event: Event {
                    id: oracle_event.id,
                    expected_outcome_time: oracle_event.expected_outcome_time,
                },
                nonces: oracle_event.nonces,
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
            nonces: oracle_event.nonces,
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

    pub fn anticipate_attestations(
        &self,
        public_key: &C::PublicKey,
        nonce_index: usize,
    ) -> Vec<C::AnticipatedAttestation> {
        C::anticipate_attestations(
            public_key,
            &self.nonces[nonce_index],
            self.event.id.n_outcomes_for_nonce(nonce_index),
        )
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

    pub fn create(event: Event, keypair: &C::KeyPair, nonces: Vec<C::PublicNonce>) -> Self {
        let oracle_event = OracleEvent::<C> { event, nonces };

        let encoded_oracle_event = oracle_event.encode_json();
        let signature = encoded_oracle_event.sign(keypair);
        Self {
            signature,
            oracle_event: encoded_oracle_event,
        }
    }

    pub fn test_instance(event: Event) -> Self {
        Self::create(
            event.clone(),
            &C::test_keypair(),
            (0..event.id.event_kind().n_nonces())
                .map(|_| C::test_nonce_keypair().into())
                .collect(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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
