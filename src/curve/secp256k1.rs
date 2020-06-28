use crate::seed::Seed;

use diesel::sql_types;
use digest::{Input, VariableOutput};
pub use schnorr_fun::{
    fun::{self, marker::*, s, Scalar, XOnly, G},
    Schnorr,
};

pub struct Secp256k1;

#[derive(
    PartialEq, Clone, Debug, FromSqlRow, AsExpression, serde::Serialize, serde::Deserialize,
)]
#[sql_type = "sql_types::Binary"]
pub struct PublicKey(XOnly<()>);

#[derive(PartialEq, Clone, Debug, FromSqlRow, AsExpression, serde::Serialize)]
#[sql_type = "sql_types::Binary"]
pub struct SchnorrScalar(Scalar<Public, Zero>);

lazy_static::lazy_static! {
    static ref SCHNORR: Schnorr = Schnorr::from_tag(b"oracle");
}

#[derive(Debug, Clone)]
pub struct KeyPair {
    pub secret_key: Scalar,
    pub public_key: PublicKey,
}

impl KeyPair {
    pub fn as_tuple(&self) -> (&Scalar, &PublicKey) {
        (&self.secret_key, &self.public_key)
    }

    pub fn new_keypair(mut scalar: Scalar) -> Self {
        let xonly = XOnly::<EvenY>::from_scalar_mul(G, &mut scalar);
        KeyPair {
            secret_key: scalar,
            public_key: PublicKey(xonly.mark::<()>()),
        }
    }

    pub fn new_nonce_keypair(mut scalar: Scalar) -> Self {
        let xonly = XOnly::<SquareY>::from_scalar_mul(G, &mut scalar);
        KeyPair {
            secret_key: scalar,
            public_key: PublicKey(xonly.mark::<()>()),
        }
    }
}

impl From<KeyPair> for PublicKey {
    fn from(keypair: KeyPair) -> Self {
        keypair.public_key
    }
}

impl super::Curve for Secp256k1 {
    type KeyPair = KeyPair;
    type PublicKey = PublicKey;
    type SchnorrScalar = SchnorrScalar;

    fn derive_keypair(seed: &Seed) -> Self::KeyPair {
        let mut hash = seed.to_blake2b_32();
        hash.input(b"secp256k1");
        let scalar = Scalar::from_slice_mod_order(&hash.vec_result())
            .expect("hash output is 32-bytes long")
            .mark::<NonZero>()
            .expect("will not be zero");
        KeyPair::new_keypair(scalar)
    }

    fn derive_nonce_keypair(seed: &Seed) -> Self::KeyPair {
        let mut hash = seed.to_blake2b_32();
        hash.input(b"secp256k1");
        let scalar = Scalar::from_slice_mod_order(&hash.vec_result())
            .expect("hash output is 32-bytes long")
            .mark::<NonZero>()
            .expect("will not be zero");
        KeyPair::new_nonce_keypair(scalar)
    }

    fn reveal_signature_s(
        signing_keypair: &Self::KeyPair,
        nonce_keypair: &Self::KeyPair,
        message: &[u8],
    ) -> Self::SchnorrScalar {
        let (r, R) = signing_keypair.as_tuple();
        let (x, X) = nonce_keypair.as_tuple();
        let c = SCHNORR.challenge(
            &R.0.clone().mark::<SquareY>(),
            &X.0.clone().mark::<EvenY>(),
            message.mark::<Public>(),
        );
        SchnorrScalar(s!(r + c * x).mark::<Public>())
    }
}

mod diesel_impl {
    use super::*;
    use diesel::{
        backend::Backend,
        deserialize::{self, *},
        serialize::{self, *},
    };
    use std::io::Write;

    impl<DB: Backend> FromSql<sql_types::Binary, DB> for PublicKey
    where
        Vec<u8>: FromSql<sql_types::Binary, DB>,
    {
        fn from_sql(bytes: Option<&DB::RawValue>) -> deserialize::Result<Self> {
            let bytes = <Vec<u8> as FromSql<sql_types::Binary, DB>>::from_sql(bytes)?;
            let mut x_only_bytes = [0u8; 32];
            x_only_bytes.copy_from_slice(&bytes[..]);
            let x_only = XOnly::from_bytes(x_only_bytes).ok_or(format!(
                "Invalid secp256k1 x-coordinate: {}",
                crate::util::to_hex(bytes.as_ref()),
            ))?;
            Ok(Self(x_only))
        }
    }

    impl<DB: Backend> FromSql<sql_types::Binary, DB> for SchnorrScalar
    where
        Vec<u8>: FromSql<sql_types::Binary, DB>,
    {
        fn from_sql(bytes: Option<&DB::RawValue>) -> deserialize::Result<Self> {
            let bytes = <Vec<u8> as FromSql<sql_types::Binary, DB>>::from_sql(bytes)?;
            let scalar = Scalar::from_slice(&bytes[..])
                .ok_or(format!(
                    "Invalid secp256k1 scalar retrieved from database: {}",
                    crate::util::to_hex(bytes.as_ref())
                ))?
                .mark::<Public>();
            Ok(Self(scalar))
        }
    }

    impl<DB: Backend> ToSql<sql_types::Binary, DB> for PublicKey {
        fn to_sql<W: Write>(&self, out: &mut Output<W, DB>) -> serialize::Result {
            ToSql::<sql_types::Binary, DB>::to_sql(self.0.as_bytes().as_ref(), out)
        }
    }

    impl<DB: Backend> ToSql<sql_types::Binary, DB> for SchnorrScalar {
        fn to_sql<W: Write>(&self, out: &mut Output<W, DB>) -> serialize::Result {
            ToSql::<sql_types::Binary, DB>::to_sql(self.0.to_bytes().as_ref(), out)
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn random_derivation_bug_on_fbsd() {
        use std::str::FromStr;
        let seed = Seed::from_str("b6bb296228cfc97eda8a6fb6c4ba935a7daec0bbea999360528c0ec1123f96f04659861e727155bfd296afdbdc5159efbf3407a0738afe34f076c369226b2d57").unwrap();
        let keychain = crate::keychain::KeyChain::new(seed);
        let nonces = keychain.nonces_for_event(&crate::event::EventId::from(
            "time/2020-06-28T16:02:00".to_string(),
        ));

        assert_eq!(
            nonces.secp256k1.secret_key,
            Scalar::<Secret>::from_str(
                "1051feef64ae176e305ed29d46bb8f95c02040bdc94fa8b0be908584008f70b0"
            )
            .unwrap(),
            "secret key is as expected"
        );

        assert_eq!(
            fun::g!(nonces.secp256k1.secret_key * G)
                .mark::<Normal>()
                .to_xonly(),
            nonces.secp256k1.public_key.0,
            "scalar mul was done to derive public key",
        );

        assert_eq!(
            nonces.secp256k1.public_key.0,
            XOnly::from_str("b63f7f4fb7873b77101ff024c95dad33c665771a470688513f17b2e521b45354")
                .unwrap(),
            "scalar mul was correct",
        );
    }
}
