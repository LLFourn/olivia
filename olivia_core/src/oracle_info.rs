use crate::Group;

pub type OracleId = alloc::string::String;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq)]
pub struct OracleInfo<C: Group> {
    pub id: OracleId,
    pub oracle_keys: OracleKeys<C>,
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct OracleKeys<C: Group> {
    pub attestation_key: C::PublicKey,
    pub announcement_key: C::PublicKey,
}

impl<C: Group> OracleInfo<C> {
    pub fn test_oracle_info() -> OracleInfo<C> {
        OracleInfo {
            id: "oracle.test".into(),
            oracle_keys: OracleKeys {
                attestation_key: C::test_keypair().into(),
                announcement_key: C::test_keypair().into(),
            },
        }
    }
}
