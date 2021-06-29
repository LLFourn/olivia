use crate::{AnnouncedEvent, Attestation, Group, OracleKeys, RawAnnouncement, PathNode};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct EventResponse<C: Group> {
    pub announcement: RawAnnouncement<C>,
    pub attestation: Option<Attestation<C>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct RootResponse<C: Group> {
    #[serde(flatten)]
    pub public_keys: OracleKeys<C>,
    #[serde(flatten)]
    pub node: PathNode,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct PathResponse {
    #[serde(flatten)]
    pub node: PathNode
}

impl<C: Group> From<AnnouncedEvent<C>> for EventResponse<C> {
    fn from(ann: AnnouncedEvent<C>) -> Self {
        EventResponse {
            announcement: ann.announcement,
            attestation: ann.attestation,
        }
    }
}
