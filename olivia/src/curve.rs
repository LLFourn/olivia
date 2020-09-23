use crate::seed::Seed;
use core::borrow::Borrow;
use olivia_secp256k1::{
    schnorr_fun::fun::{
        digest::{Update, VariableOutput},
        marker::*,
        Scalar, XOnly,
    },
    Secp256k1, SCHNORR,
};

pub type SchnorrImpl = Secp256k1;
pub type PublicKey = olivia_secp256k1::PublicKey;
pub type PublicNonce = olivia_secp256k1::PublicNonce;
pub type SigScalar = olivia_secp256k1::SigScalar;
pub type Signature = olivia_secp256k1::Signature;

pub trait DeriveKeyPair: olivia_core::Schnorr {
    fn derive_keypair(seed: &Seed) -> Self::KeyPair;
    fn derive_nonce_keypair(seed: &Seed) -> Self::NonceKeyPair;
}

impl DeriveKeyPair for Secp256k1 {
    fn derive_keypair(seed: &Seed) -> Self::KeyPair {
        let mut hash = seed.to_blake2b_32();
        hash.update(b"secp256k1");
        let x = Scalar::from_slice_mod_order(&hash.finalize_boxed().borrow())
            .expect("hash output is 32-bytes long")
            .mark::<NonZero>()
            .expect("will not be zero");
        SCHNORR.new_keypair(x)
    }

    fn derive_nonce_keypair(seed: &Seed) -> Self::NonceKeyPair {
        let mut hash = seed.to_blake2b_32();
        hash.update(b"secp256k1");
        let mut r = Scalar::from_slice_mod_order(&hash.finalize_boxed().borrow())
            .expect("hash output is 32-bytes long")
            .mark::<NonZero>()
            .expect("will not be zero");

        let R = XOnly::from_scalar_mul(&SCHNORR.G(), &mut r);

        (r, R)
    }
}
