use crate::schnorr::Schnorr;

pub type OracleId = alloc::string::String;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq)]
pub struct OracleInfo<C: Schnorr> {
    pub id: OracleId,
    pub public_key: C::PublicKey
}
