use crate::Group;

pub type OracleId = alloc::string::String;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq)]
pub struct OracleInfo<C: Group> {
    pub id: OracleId,
    #[serde(flatten)]
    pub oracle_keys: OracleKeys<C>,
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct OracleKeys<C: Group> {
    pub attestation_key: C::PublicKey,
    pub announcement_key: C::PublicKey,
}
