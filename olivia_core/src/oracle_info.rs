use crate::Group;

pub type OracleId = alloc::string::String;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq)]
pub struct OracleInfo<C: Group> {
    pub id: OracleId,
    pub announcement_key: C::PublicKey,
    pub attestation_key: C::PublicKey,
}
