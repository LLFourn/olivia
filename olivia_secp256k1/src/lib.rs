#![allow(non_snake_case)]
pub use schnorr_fun::{self, fun, KeyPair};
use schnorr_fun::{
    fun::{marker::*, nonce::Deterministic, s, g, Point, Scalar, XOnly, G},
    Message, Schnorr,
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
pub struct AttestScalar(Scalar<Public, NonZero>);

olivia_core::impl_display_debug_serialize_tosql! {
    fn to_bytes(scalar: &AttestScalar) -> [u8;32] {
        scalar.0.to_bytes()
    }
}

olivia_core::impl_fromstr_deserailize_fromsql! {
    name => "secp256k1 scalar",
    fn from_bytes(bytes: [u8;32]) ->  Option<AttestScalar> {
        Scalar::from_bytes(bytes).and_then(|scalar| scalar.mark::<NonZero>()).map(|s| AttestScalar(s.mark::<Public>()))
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
    pub static ref SCHNORR: Schnorr<Sha256, Deterministic<Sha256>> = Schnorr::new(Deterministic::<Sha256>::default());
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

impl From<XOnly> for PublicNonce {
    fn from(x: XOnly) -> Self {
        Self(x)
    }
}

impl From<PublicNonce> for XOnly {
    fn from(x: PublicNonce) -> Self {
        x.0
    }
}

impl From<AttestScalar> for Scalar<Public, NonZero> {
    fn from(att_scalar: AttestScalar) -> Self {
        att_scalar.0
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

impl olivia_core::Group for Secp256k1 {
    type KeyPair = KeyPair;
    type PublicKey = PublicKey;
    type PublicNonce = PublicNonce;
    type NonceKeyPair = (Scalar, XOnly);
    type Signature = Signature;
    type AttestScalar = AttestScalar;
    type AnticipatedAttestation = Point<Jacobian, Public, NonZero>;

    fn name() -> &'static str {
        "secp256k1"
    }

    fn verify_announcement_signature(
        public_key: &Self::PublicKey,
        message: &[u8],
        sig: &Self::Signature,
    ) -> bool {
        let public_key = public_key.0.clone();
        let verification_key = public_key.to_point();
        SCHNORR.verify(
            &verification_key,
            Message::<Public>::plain("DLC/announcement", &message[..]),
            &sig.0,
        )
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

    fn reveal_attest_scalar(
        signing_key: &Self::KeyPair,
        nonce_key: Self::NonceKeyPair,
        index: u32,
    ) -> Self::AttestScalar {
        let r = nonce_key.0;
        let c = Scalar::from(index);
        AttestScalar(s!(r + c * { signing_key.secret_key() } ).mark::<(Public, NonZero)>().expect("will not be zero since public_key and public_nonce are independent"))
    }

    fn anticipate_attestations(
        public_key: &Self::PublicKey,
        public_nonce: &Self::PublicNonce,
        n_outcomes: u32,
    ) -> Vec<Self::AnticipatedAttestation> {
        let X = public_key.0.to_point();
        let R = public_nonce.0.to_point().mark::<Jacobian>();
        (0..n_outcomes).scan( R, |C: &mut Point<Jacobian>, _| {
            *C = g!({ *C } + X).mark::<NonZero>().expect("will not be zero since public_key and public_nonce are independent");
            Some(*C)
        }).collect()
    }

    fn sign_announcement(keypair: &Self::KeyPair, announcement: &[u8]) -> Self::Signature {
        Signature(SCHNORR.sign(
            keypair,
            Message::<Public>::plain("DLC/announcement", announcement),
        ))
    }

    fn verify_attest_scalar(
        public_key: &Self::PublicKey,
        public_nonce: &Self::PublicNonce,
        index: u32,
        attest_scalar: &Self::AttestScalar,
    ) -> bool {
        let s = &attest_scalar.0;
        let R = public_nonce.0.to_point();
        let X = public_key.0.to_point();
        let c = Scalar::from(index);
        g!(s * G) == g!(R + c * X)
    }
}

olivia_core::impl_deserialize_curve!(Secp256k1);
