use crate::{AnnouncedEvent, Attestation, EventId, RawAnnouncement, Schnorr};
use alloc::{string::String, vec::Vec};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PathResponse<C: Schnorr> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_key: Option<C::PublicKey>,
    pub events: Vec<EventId>,
    pub children: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EventResponse<C: Schnorr> {
    pub announcement: RawAnnouncement<C>,
    pub attestation: Option<Attestation<C>>,
}

impl<C: Schnorr> From<AnnouncedEvent<C>> for EventResponse<C> {
    fn from(ann: AnnouncedEvent<C>) -> Self {
        EventResponse {
            announcement: ann.announcement,
            attestation: ann.attestation,
        }
    }
}
