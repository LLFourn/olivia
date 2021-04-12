use crate::{AnnouncedEvent, Attestation, EventId, Group, OracleKeys, RawAnnouncement};
use alloc::{string::String, vec::Vec};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PathResponse {
    pub events: Vec<EventId>,
    pub children: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EventResponse<C: Group> {
    pub announcement: RawAnnouncement<C>,
    pub attestation: Option<Attestation<C>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RootResponse<C: Group> {
    #[serde(flatten)]
    pub public_keys: OracleKeys<C>,
    #[serde(flatten)]
    pub path_response: PathResponse,
}

impl<C: Group> From<AnnouncedEvent<C>> for EventResponse<C> {
    fn from(ann: AnnouncedEvent<C>) -> Self {
        EventResponse {
            announcement: ann.announcement,
            attestation: ann.attestation,
        }
    }
}
