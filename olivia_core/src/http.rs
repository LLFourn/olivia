use alloc::vec::Vec;
use alloc::string::String;
use crate::{EventId, Schnorr, Announcement, Attestation, AnnouncedEvent};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PathResponse<C: Schnorr> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_key: Option<C::PublicKey>,
    pub events: Vec<EventId>,
    pub children: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EventResponse<C: Schnorr> {
    pub id: EventId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_outcome_time: Option<chrono::NaiveDateTime>,
    pub announcement: Announcement<C>,
    pub attestation: Option<Attestation<C>>,
}

impl<C: Schnorr> From<AnnouncedEvent<C>> for EventResponse<C> {
    fn from(ann: AnnouncedEvent<C>) -> Self {
        EventResponse {
            id: ann.event.id,
            expected_outcome_time: ann.event.expected_outcome_time,
            announcement: ann.announcement,
            attestation: ann.attestation,
        }
    }
}
