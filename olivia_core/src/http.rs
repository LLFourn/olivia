use crate::{AnnouncedEvent, Attestation, GetPath, Group, OracleKeys, RawAnnouncement};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(bound = "C: Group")]
pub struct EventResponse<C: Group> {
    pub announcement: RawAnnouncement<C>,
    pub attestation: Option<Attestation<C>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(bound = "C: Group")]
pub struct RootResponse<C: Group> {
    #[serde(flatten)]
    pub public_keys: OracleKeys<C>,
    #[serde(flatten)]
    pub node: GetPath,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct PathResponse {
    #[serde(flatten)]
    pub node: GetPath,
}

impl<C: Group> From<AnnouncedEvent<C>> for EventResponse<C> {
    fn from(ann: AnnouncedEvent<C>) -> Self {
        EventResponse {
            announcement: ann.announcement,
            attestation: ann.attestation,
        }
    }
}
