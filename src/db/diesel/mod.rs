use crate::{
    event::{self, EventId, Scalars},
    oracle,
};
use diesel::Insertable;
use schema::{attestations, events, meta, nonces};
use std::convert::TryFrom;

pub mod postgres;
pub mod schema;

#[derive(Identifiable, QueryableByName, Queryable, Debug, Insertable, Clone, PartialEq)]
#[table_name = "events"]
struct Event {
    id: String,
    path: Vec<String>,
    human_url: Option<String>,
    kind: event::EventKind,
    expected_outcome_time: chrono::NaiveDateTime,
}

impl From<Event> for event::Event {
    fn from(event: Event) -> Self {
        event::Event {
            id: event.id.into(),
            human_url: event.human_url,
            kind: event.kind,
            expected_outcome_time: event.expected_outcome_time,
        }
    }
}

impl From<event::Event> for Event {
    fn from(event: event::Event) -> Self {
        Event {
            path: event.id.path(),
            id: event.id.into(),
            human_url: event.human_url,
            kind: event.kind,
            expected_outcome_time: event.expected_outcome_time,
        }
    }
}

#[derive(Identifiable, Queryable, Associations, Debug, Insertable, Clone, PartialEq)]
#[belongs_to(Event)]
#[table_name = "nonces"]
#[primary_key(event_id)]
struct Nonce {
    pub event_id: String,
    pub ed25519: crate::curve::ed25519::PublicKey,
    pub secp256k1: crate::curve::secp256k1::PublicKey,
}

impl Nonce {
    fn from_core_domain(
        event_id: EventId,
        event::Nonce { ed25519, secp256k1 }: event::Nonce,
    ) -> Self {
        Self {
            event_id: event_id.into(),
            ed25519,
            secp256k1,
        }
    }
}

impl From<Nonce> for event::Nonce {
    fn from(nonce: Nonce) -> Self {
        event::Nonce {
            ed25519: nonce.ed25519,
            secp256k1: nonce.secp256k1,
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
        event::Attestation {
            outcome,
            time,
            scalars,
            ..
        }: event::Attestation,
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

impl From<Attestation> for event::Attestation {
    fn from(
        Attestation {
            outcome,
            ed25519,
            secp256k1,
            time,
            ..
        }: Attestation,
    ) -> Self {
        event::Attestation::new(outcome, time, Scalars { ed25519, secp256k1 })
    }
}

#[derive(Debug, Clone, PartialEq, Queryable)]
struct ObservedEvent {
    #[diesel(embed)]
    event: Event,
    #[diesel(embed)]
    nonce: Nonce,
    #[diesel(embed)]
    attestation: Option<Attestation>,
}

impl From<ObservedEvent> for event::ObservedEvent {
    fn from(
        ObservedEvent {
            event,
            nonce,
            attestation,
        }: ObservedEvent,
    ) -> event::ObservedEvent {
        event::ObservedEvent {
            event: event.into(),
            nonce: nonce.into(),
            attestation: attestation.map(|a| a.into()),
        }
    }
}

impl From<event::ObservedEvent> for ObservedEvent {
    fn from(
        event::ObservedEvent {
            event,
            nonce,
            attestation,
        }: event::ObservedEvent,
    ) -> Self {
        Self {
            event: event.clone().into(),
            nonce: Nonce::from_core_domain(event.id.clone(), nonce),
            attestation: attestation.map(|a| Attestation::from_core_domain(event.id.clone(), a)),
        }
    }
}

// struct OraclePubkeys {
//     pub ed25519: ed25519::PublicKey,
//     pub secp256k1: secp256k1::PublicKey,
// }

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
