use crate::{
    core::{EventId, EventOutcome, Nonce, Scalars},
    curve::{ed25519, secp256k1, Curve, Ed25519, Secp256k1},
    oracle::OraclePubkeys,
    seed::Seed,
};

pub struct KeyChain {
    ed25519_keypair: ed25519::KeyPair,
    secp256k1_keypair: secp256k1::KeyPair,
    event_seed: Seed,
}

impl KeyChain {
    pub fn new(seed: Seed) -> Self {
        let key_seed = seed.child(b"oracle-key");
        let secp256k1_keypair = Secp256k1::derive_keypair(&key_seed);
        let ed25519_keypair = Ed25519::derive_keypair(&key_seed);
        Self {
            ed25519_keypair,
            secp256k1_keypair,
            event_seed: seed.child(b"oracle-events"),
        }
    }

    pub fn oracle_pubkeys(&self) -> OraclePubkeys {
        OraclePubkeys {
            ed25519: self.ed25519_keypair.clone().into(),
            // TODO: Make this symmetric
            secp256k1: self.secp256k1_keypair.public_key().clone().into(),
        }
    }

    pub fn nonces_for_event(&self, event_id: &EventId) -> NonceKeyPairs {
        let event_idx = self.event_seed.child(event_id.as_bytes());
        NonceKeyPairs {
            ed25519: Ed25519::derive_nonce_keypair(&event_idx).into(),
            secp256k1: Secp256k1::derive_nonce_keypair(&event_idx).into(),
        }
    }

    pub fn scalars_for_event_outcome(&self, outcome: &EventOutcome) -> Scalars {
        let outcome_long_id = outcome.attestation_string();
        let event_idx = self.event_seed.child(outcome.event_id.as_bytes());

        let ed25519_s = Ed25519::reveal_signature_s(
            &self.ed25519_keypair,
            &Ed25519::derive_nonce_keypair(&event_idx),
            outcome_long_id.as_bytes(),
        );

        let secp256k1_s = Secp256k1::reveal_signature_s(
            &self.secp256k1_keypair,
            &Secp256k1::derive_nonce_keypair(&event_idx),
            outcome_long_id.as_bytes(),
        );

        Scalars {
            ed25519: ed25519_s,
            secp256k1: secp256k1_s,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NonceKeyPairs {
    pub ed25519: <Ed25519 as Curve>::NonceKeyPair,
    pub secp256k1: <Secp256k1 as Curve>::NonceKeyPair,
}

impl From<NonceKeyPairs> for Nonce {
    fn from(kp: NonceKeyPairs) -> Self {
        Nonce {
            ed25519: kp.ed25519.into(),
            secp256k1: kp.secp256k1.into(),
        }
    }
}
