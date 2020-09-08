use crate::{
    core::{self, EventId},
    curve::*,
};
use diesel::Insertable;
use schema::{announcements, attestations, events, meta, tree};

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
            node: event.id.as_path().as_str().into(),
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
    pub nonce: PublicNonce,
    pub signature: Signature,
}

impl Announcement {
    fn from_core_domain(
        event_id: EventId,
        core::Announcement { signature, nonce }: core::Announcement<SchnorrImpl>,
    ) -> Self {
        Self {
            event_id: event_id.into(),
            signature,
            nonce,
        }
    }
}

impl From<Announcement> for core::Announcement<SchnorrImpl> {
    fn from(
        Announcement {
            signature, nonce, ..
        }: Announcement,
    ) -> Self {
        Self { signature, nonce }
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
    pub scalar: SigScalar,
}

impl Attestation {
    pub fn from_core_domain(
        event_id: EventId,
        core::Attestation {
            outcome,
            time,
            scalar,
            ..
        }: core::Attestation<SchnorrImpl>,
    ) -> Self {
        Attestation {
            event_id: event_id.into(),
            outcome,
            time,
            scalar,
        }
    }
}

impl From<Attestation> for core::Attestation<SchnorrImpl> {
    fn from(
        Attestation {
            outcome,
            time,
            scalar,
            ..
        }: Attestation,
    ) -> Self {
        core::Attestation::new(outcome, time, scalar)
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

impl From<AnnouncedEvent> for core::AnnouncedEvent<SchnorrImpl> {
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

impl From<core::AnnouncedEvent<SchnorrImpl>> for AnnouncedEvent {
    fn from(
        core::AnnouncedEvent {
            event,
            announcement,
            attestation,
        }: core::AnnouncedEvent<SchnorrImpl>,
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

#[derive(serde::Serialize, serde::Deserialize)]
struct PublicKeyMeta {
    curve: SchnorrImpl,
    public_key: PublicKey,
}
