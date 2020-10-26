use core::marker::PhantomData;
use crate::{Attestation, Descriptor, Event, EventId, Schnorr};
use alloc::vec::Vec;
use alloc::string::String;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RawAnnouncement<C: Schnorr> {
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

impl<C: Schnorr> RawOracleEvent<C> {
    #[must_use]
    pub fn verify(&self, oracle_public_key: &C::PublicKey, announcement_signature: &C::Signature) -> bool {
        C::verify_signature(oracle_public_key, self.payload.as_bytes(), announcement_signature)
    }

    pub fn decode(&self) -> Option<OracleEvent<C>> {
        self.payload.decode()
    }

    pub fn sign(&self, keypair: &C::KeyPair) -> C::Signature {
        C::sign(keypair, self.payload.as_bytes())
    }


    pub fn as_bytes(&self) -> &[u8] {
        self.payload.as_bytes()
    }

    pub unsafe fn from_json_bytes_unchecked(bytes: Vec<u8>) -> Self {
        Self {
            payload: RawOracleEventEncoding::Json(String::from_utf8_unchecked(bytes)),
            curve: PhantomData,
        }
    }
}


#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case", tag = "encoding", content = "data")]
enum RawOracleEventEncoding {
    Json(String)
}

impl RawOracleEventEncoding {
    fn decode<C: Schnorr>(&self) -> Option<OracleEvent<C>> {
        use RawOracleEventEncoding::*;
        match self {
            Json(string) => serde_json::from_str(string).ok(),
        }
    }

    fn as_bytes(&self) -> &[u8] {
        use RawOracleEventEncoding::*;
        match self {
            Json(string) => string.as_bytes()
        }
    }
}



#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OracleEvent<C: Schnorr> {
    #[serde(flatten)]
    pub event: Event,
    pub descriptor: Descriptor,
    pub nonces: Vec<C::PublicNonce>,
}


impl<C: Schnorr> OracleEvent<C> {
    fn encode_json(&self) -> RawOracleEvent<C> {
        RawOracleEvent {
            payload: RawOracleEventEncoding::Json(serde_json::to_string(self).unwrap()),
            curve: PhantomData,
        }
    }
}

impl<C: Schnorr> RawAnnouncement<C> {
    #[must_use]
    pub fn verify_against_id(
        &self,
        event_id: &EventId,
        oracle_public_key: &C::PublicKey,
    ) -> Option<OracleEvent<C>> {

        if !self.oracle_event.verify(oracle_public_key, &self.signature) {
            return None;
        }

        let oracle_event = self.oracle_event.decode()?;

        if event_id.descriptor::<C>() != oracle_event.descriptor {
            return None;
        }

        Some(oracle_event)
    }

    pub fn create(event: Event, keypair: &C::KeyPair, nonces: Vec<C::PublicNonce>) -> Self {
        let descriptor = event.id.descriptor::<C>();

        let oracle_event = OracleEvent::<C> {
            event,
            descriptor,
            nonces,
        };

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
            (0..event.id.event_kind().n_fragments())
                .map(|_| C::test_nonce_keypair().into())
                .collect(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AnnouncedEvent<C: Schnorr> {
    pub event: Event,
    pub announcement: RawAnnouncement<C>,
    pub attestation: Option<Attestation<C>>,
}

impl<C: Schnorr> AnnouncedEvent<C> {

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
