use crate::{
    core::{self, EventId, Scalars},
    curve::{ed25519::Ed25519, secp256k1::Secp256k1, Curve},
    oracle,
};
use diesel::Insertable;
use schema::{announcements, attestations, events, meta, tree};
use std::convert::TryFrom;

pub mod postgres;
pub mod schema;

#[derive(Identifiable, QueryableByName, Queryable, Debug, Insertable, Clone, PartialEq)]
#[table_name = "events"]
struct Event {
    id: EventId,
    node: String,
    expected_outcome_time: Option<chrono::NaiveDateTime>,
}

#[derive(Identifiable, QueryableByName, Queryable, Debug, Insertable, Clone, PartialEq)]
#[table_name = "tree"]
struct Node {
    pub id: String,
    pub parent: Option<String>,
}

impl From<Event> for core::Event {
    fn from(event: Event) -> Self {
        core::Event {
            id: event.id.into(),
            expected_outcome_time: event.expected_outcome_time,
        }
    }
}

impl From<core::Event> for Event {
    fn from(event: core::Event) -> Self {
        Event {
            node: event.id.node().as_str().into(),
            id: event.id.into(),
            expected_outcome_time: event.expected_outcome_time,
        }
    }
}

#[derive(Identifiable, Queryable, Associations, Debug, Insertable, Clone, PartialEq)]
#[belongs_to(Event)]
#[table_name = "announcements"]
#[primary_key(event_id)]
struct Announcement {
    pub event_id: String,
    pub ed25519_nonce: <Ed25519 as Curve>::PublicNonce,
    pub ed25519_signature: <Ed25519 as Curve>::SchnorrSignature,
    pub secp256k1_nonce: <Secp256k1 as Curve>::PublicNonce,
    pub secp256k1_signature: <Secp256k1 as Curve>::SchnorrSignature,
}

impl Announcement {
    fn from_core_domain(
        event_id: EventId,
        core::Announcement { ed25519, secp256k1 }: core::Announcement,
    ) -> Self {
        Self {
            event_id: event_id.into(),
            ed25519_nonce: ed25519.nonce,
            ed25519_signature: ed25519.signature,
            secp256k1_nonce: secp256k1.nonce,
            secp256k1_signature: secp256k1.signature,
        }
    }
}

impl From<Announcement> for core::Announcement {
    fn from(ann: Announcement) -> Self {
        Self {
            ed25519: core::NonceAndSig {
                nonce: ann.ed25519_nonce,
                signature: ann.ed25519_signature,
            },
            secp256k1: core::NonceAndSig {
                nonce: ann.secp256k1_nonce,
                signature: ann.secp256k1_signature,
            },
        }
    }
}

#[derive(Identifiable, Associations, Queryable, Insertable, Debug, Clone, PartialEq)]
#[belongs_to(Event)]
#[table_name = "attestations"]
#[primary_key(event_id)]
struct Attestation {
    pub event_id: String,
    pub outcome: String,
    pub time: chrono::NaiveDateTime,
    pub ed25519: crate::curve::ed25519::SchnorrScalar,
    pub secp256k1: crate::curve::secp256k1::SchnorrScalar,
}

impl Attestation {
    pub fn from_core_domain(
        event_id: EventId,
        core::Attestation {
            outcome,
            time,
            scalars,
            ..
        }: core::Attestation,
    ) -> Self {
        Attestation {
            time: time,
            event_id: event_id.into(),
            outcome: outcome,
            ed25519: scalars.ed25519,
            secp256k1: scalars.secp256k1,
        }
    }
}

impl From<Attestation> for core::Attestation {
    fn from(
        Attestation {
            outcome,
            ed25519,
            secp256k1,
            time,
            ..
        }: Attestation,
    ) -> Self {
        core::Attestation::new(outcome, time, Scalars { ed25519, secp256k1 })
    }
}

#[derive(Debug, Clone, PartialEq, Queryable)]
struct AnnouncedEvent {
    #[diesel(embed)]
    event: Event,
    #[diesel(embed)]
    announcement: Announcement,
    #[diesel(embed)]
    attestation: Option<Attestation>,
}

impl From<AnnouncedEvent> for core::AnnouncedEvent {
    fn from(
        AnnouncedEvent {
            event,
            announcement,
            attestation,
        }: AnnouncedEvent,
    ) -> Self {
        core::AnnouncedEvent {
            event: event.into(),
            announcement: announcement.into(),
            attestation: attestation.map(Into::into),
        }
    }
}

impl From<core::AnnouncedEvent> for AnnouncedEvent {
    fn from(
        core::AnnouncedEvent {
            event,
            announcement,
            attestation,
        }: core::AnnouncedEvent,
    ) -> Self {
        Self {
            event: event.clone().into(),
            announcement: Announcement::from_core_domain(event.id.clone(), announcement),
            attestation: attestation.map(|a| Attestation::from_core_domain(event.id.clone(), a)),
        }
    }
}

#[derive(Identifiable, QueryableByName, Queryable, Debug, Insertable, Clone, PartialEq)]
#[table_name = "meta"]
#[primary_key(key)]
struct MetaRow {
    key: String,
    value: serde_json::Value,
}

impl TryFrom<MetaRow> for oracle::OraclePubkeys {
    type Error = serde_json::Error;
    fn try_from(meta: MetaRow) -> Result<Self, Self::Error> {
        serde_json::from_value::<oracle::OraclePubkeys>(meta.value)
    }
}

impl From<oracle::OraclePubkeys> for MetaRow {
    fn from(keys: oracle::OraclePubkeys) -> Self {
        MetaRow {
            key: "oracle_pubkeys".into(),
            value: serde_json::to_value(keys).unwrap(),
        }
    }
}
