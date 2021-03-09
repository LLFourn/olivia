use crate::{AnnouncedEvent, Attestation, EventId, RawAnnouncement, Schnorr};
use alloc::{string::String, vec::Vec};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PathResponse {
    pub events: Vec<EventId>,
    pub children: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EventResponse<C: Schnorr> {
    pub announcement: RawAnnouncement<C>,
    pub attestation: Option<Attestation<C>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RootResponse<C: Schnorr> {
    pub public_key: C::PublicKey,
    #[serde(flatten)]
    pub path_response: PathResponse,
}

impl<C: Schnorr> From<AnnouncedEvent<C>> for EventResponse<C> {
    fn from(ann: AnnouncedEvent<C>) -> Self {
        EventResponse {
            announcement: ann.announcement,
            attestation: ann.attestation,
        }
    }
}
