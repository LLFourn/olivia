#![allow(non_snake_case)]
use olivia_core::EventId;
use olivia_core::Outcome;
pub use schnorr_fun;
use schnorr_fun::{
    fun::{digest::Digest, marker::*, nonce::Deterministic, s, Point, Scalar, XOnly, G},
    KeyPair, MessageKind, Schnorr,
};
use sha2::Sha256;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Secp256k1;

#[derive(PartialEq, Clone)]
#[cfg_attr(feature = "diesel", derive(diesel::FromSqlRow, diesel::AsExpression))]
#[cfg_attr(feature = "diesel", sql_type = "diesel::sql_types::Binary")]
pub struct PublicKey(XOnly);

olivia_core::impl_display_debug_serialize_tosql! {
    fn to_bytes(pk: &PublicKey) -> &[u8;32] {
        pk.0.as_bytes()
    }
}

olivia_core::impl_fromstr_deserailize_fromsql! {
    name => "secp256k1 xonly public key",
    fn from_bytes(bytes: [u8;32]) ->  Option<PublicKey> {
        XOnly::from_bytes(bytes).map(PublicKey)
    }
}

#[derive(PartialEq, Clone)]
#[cfg_attr(feature = "diesel", derive(diesel::FromSqlRow, diesel::AsExpression))]
#[cfg_attr(feature = "diesel", sql_type = "diesel::sql_types::Binary")]
pub struct PublicNonce(XOnly);

olivia_core::impl_display_debug_serialize_tosql! {
    fn to_bytes(pn: &PublicNonce) -> &[u8;32] {
        pn.0.as_bytes()
    }
}

olivia_core::impl_fromstr_deserailize_fromsql! {
    name => "secp256k1 xonly public nonce",
    fn from_bytes(bytes: [u8;32]) ->  Option<PublicNonce> {
        XOnly::from_bytes(bytes).map(PublicNonce)
    }
}

#[derive(PartialEq, Clone)]
#[cfg_attr(feature = "diesel", derive(diesel::FromSqlRow, diesel::AsExpression))]
#[cfg_attr(feature = "diesel", sql_type = "diesel::sql_types::Binary")]
pub struct SigScalar(Scalar<Public, Zero>);

olivia_core::impl_display_debug_serialize_tosql! {
    fn to_bytes(scalar: &SigScalar) -> [u8;32] {
        scalar.0.to_bytes()
    }
}

olivia_core::impl_fromstr_deserailize_fromsql! {
    name => "secp256k1 scalar",
    fn from_bytes(bytes: [u8;32]) ->  Option<SigScalar> {
        Scalar::from_bytes(bytes).map(|s| SigScalar(s.mark::<Public>()))
    }
}

#[derive(PartialEq, Clone)]
#[cfg_attr(feature = "diesel", derive(diesel::FromSqlRow, diesel::AsExpression))]
#[cfg_attr(feature = "diesel", sql_type = "diesel::sql_types::Binary")]
pub struct Signature(schnorr_fun::Signature);

olivia_core::impl_display_debug_serialize_tosql! {
    fn to_bytes(sig: &Signature) -> [u8;64] {
        sig.0.to_bytes()
    }
}

olivia_core::impl_fromstr_deserailize_fromsql! {
    name => "bip340 schnorr signature",
    fn from_bytes(bytes: [u8;64]) ->  Option<Signature> {
        schnorr_fun::Signature::from_bytes(bytes).map(Signature)
    }
}

lazy_static::lazy_static! {
    pub static ref SCHNORR: Schnorr<Sha256, Deterministic<Sha256>> = Schnorr::new(Deterministic::<Sha256>::default(), MessageKind::Prehashed);
}

impl From<XOnly> for PublicKey {
    fn from(x: XOnly) -> Self {
        Self(x)
    }
}

impl From<PublicKey> for XOnly {
    fn from(x: PublicKey) -> Self {
        x.0
    }
}

impl From<PublicNonce> for XOnly {
    fn from(x: PublicNonce) -> Self {
        x.0
    }
}

impl From<KeyPair> for PublicKey {
    fn from(kp: KeyPair) -> Self {
        let (_, pk) = kp.into();
        PublicKey(pk)
    }
}

impl From<(Scalar, XOnly)> for PublicNonce {
    fn from(kp: (Scalar, XOnly)) -> Self {
        Self(kp.1)
    }
}

impl olivia_core::Schnorr for Secp256k1 {
    type KeyPair = KeyPair;
    type PublicKey = PublicKey;
    type PublicNonce = PublicNonce;
    type SigScalar = SigScalar;
    type NonceKeyPair = (Scalar, XOnly);
    type Signature = Signature;

    fn name() -> &'static str {
        "secp256k1"
    }

    fn reveal_signature_s(
        signing_keypair: &Self::KeyPair,
        nonce_keypair: Self::NonceKeyPair,
        message: &[u8],
    ) -> Self::SigScalar {
        let (x, X) = signing_keypair.as_tuple();
        let (r, R) = nonce_keypair;
        let message = Digest::chain(Sha256::default(), message).finalize();
        let c = SCHNORR.challenge(&R, X, (&message[..]).mark::<Public>());
        let s = s!(r + c * x);
        SigScalar(s.mark::<Public>())
    }

    fn signature_from_scalar_and_nonce(
        scalar: Self::SigScalar,
        nonce: Self::PublicNonce,
    ) -> Self::Signature {
        Signature(schnorr_fun::Signature {
            R: nonce.0,
            s: scalar.0,
        })
    }

    fn verify_signature(
        public_key: &Self::PublicKey,
        message: &[u8],
        sig: &Self::Signature,
    ) -> bool {
        let public_key = public_key.0.clone();
        let message = Digest::chain(Sha256::default(), message).finalize();
        let verification_key = public_key.to_point();
        SCHNORR.verify(&verification_key, (&message[..]).mark::<Public>(), &sig.0)
    }

    fn sign(keypair: &Self::KeyPair, message: &[u8]) -> Self::Signature {
        let message = Digest::chain(Sha256::default(), message).finalize();
        Signature(SCHNORR.sign(keypair, (&message[..]).mark::<Public>()))
    }

    fn test_keypair() -> Self::KeyPair {
        SCHNORR.new_keypair(
            Scalar::from_bytes_mod_order([42u8; 32])
                .mark::<NonZero>()
                .unwrap(),
        )
    }

    fn test_nonce_keypair() -> Self::NonceKeyPair {
        let mut r = Scalar::from_bytes_mod_order([84u8; 32])
            .mark::<NonZero>()
            .unwrap();
        let R = XOnly::from_scalar_mul(G, &mut r);
        (r, R)
    }
}

pub fn anticipate_signature(
    public_key: &Point<EvenY>,
    nonce: &Point<EvenY>,
    event_id: &EventId,
    outcome: &Outcome,
) -> Point<Jacobian, Public, Zero> {
    let mut hash = WriteDigest(Sha256::default());
    hash.0.update(event_id.as_str().as_bytes());
    hash.0.update(b"=");
    outcome.write_to(&mut hash).expect("cannot fail");
    let hashed_message = hash.0.finalize();
    SCHNORR
        .anticipate_signature(
            public_key,
            nonce,
            hashed_message.as_slice().mark::<Public>(),
        )
        .mark::<Zero>()
}

struct WriteDigest<D>(D);

impl<D: Digest> core::fmt::Write for WriteDigest<D> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.0.update(s.as_bytes());
        Ok(())
    }
}

olivia_core::impl_deserialize_curve!(Secp256k1);
