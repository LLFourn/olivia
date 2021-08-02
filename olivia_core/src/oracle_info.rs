use crate::Group;

pub type OracleId = alloc::string::String;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq)]
#[serde(bound = "C: Group")]
pub struct OracleInfo<C: Group> {
    pub id: OracleId,
    pub oracle_keys: OracleKeys<C>,
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
#[serde(bound = "C: Group")]
pub struct OracleKeys<C: Group> {
    pub olivia_v1: Option<C::PublicKey>,
    pub ecdsa_v1: Option<C::PublicKey>,
    pub announcement: C::PublicKey,
    pub group: C,
}

impl<C: Group> OracleInfo<C> {
    pub fn test_oracle_info() -> OracleInfo<C> {
        OracleInfo {
            id: "oracle.test".into(),
            oracle_keys: OracleKeys {
                olivia_v1: Some(C::test_keypair().into()),
                ecdsa_v1: Some(C::test_keypair().into()),
                announcement: C::test_keypair().into(),
                group: C::default()
            },
        }
    }
}
