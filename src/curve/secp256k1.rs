use crate::seed::Seed;

use diesel::sql_types;
use digest::{Input, VariableOutput};
pub use schnorr_fun::{
    fun::{self, marker::*, s, Scalar, XOnly, G},
    Schnorr, Signature,
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
    type SchnorrSignature = Signature;

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
        let (x, X) = signing_keypair.as_tuple();
        let (r, R) = nonce_keypair.as_tuple();
        let c = SCHNORR.challenge(
            &R.0.clone().mark::<SquareY>(),
            &X.0.clone().mark::<EvenY>(),
            message.mark::<Public>(),
        );
        SchnorrScalar(s!(r + c * x).mark::<Public>())
    }

    fn signature_from_scalar_and_nonce(
        scalar: Self::SchnorrScalar,
        nonce: Self::PublicKey,
    ) -> Self::SchnorrSignature {
        Signature {
            R: nonce.0.mark::<SquareY>(),
            s: scalar.0,
        }
    }

    fn verify_signature(
        public_key: &Self::PublicKey,
        message: &[u8],
        sig: &Self::SchnorrSignature,
    ) -> bool {
        let public_key = public_key.0.clone().mark::<EvenY>();
        let message = message.mark::<Public>();
        let verification_key = public_key.to_point();
        SCHNORR.verify(&verification_key, message, sig)
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
