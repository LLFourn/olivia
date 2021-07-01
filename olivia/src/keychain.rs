use crate::seed::Seed;
use digest::{Update, VariableOutput};
use olivia_core::{Event, EventId, Group, OracleKeys, RawAnnouncement, StampedOutcome};
use std::borrow::Borrow;

pub struct KeyChain<C: Group> {
    announcement_keypair: C::KeyPair,
    attestation_keypair: C::KeyPair,
    event_seed: Seed,
}

impl<C: Group> KeyChain<C> {
    pub fn new(seed: Seed) -> Self {
        let seed = seed.child(C::name().as_bytes());
        let announcement_keypair = {
            let seed = seed.child(b"announcement-key");
            let hash = seed.to_blake2b_var(C::KEY_MATERIAL_LEN);
            C::keypair_from_secret_bytes(hash.finalize_boxed().borrow())
        };
        let attestation_keypair = {
            let seed = seed.child(b"attestation-key");
            let hash = seed.to_blake2b_var(C::KEY_MATERIAL_LEN);
            C::keypair_from_secret_bytes(hash.finalize_boxed().borrow())
        };
        Self {
            event_seed: seed.child(b"oracle-events"),
            announcement_keypair,
            attestation_keypair,
        }
    }

    pub fn oracle_public_keys(&self) -> OracleKeys<C> {
        OracleKeys {
            attestation_key: self.attestation_keypair.clone().into(),
            announcement_key: self.announcement_keypair.clone().into(),
        }
    }

    pub fn nonces_for_event(&self, event_id: &EventId) -> Vec<C::NonceKeyPair> {
        let event_seed = self.event_seed.child(event_id.as_bytes());
        let n = event_id.event_kind().n_nonces();
        let hash = event_seed.to_blake2b_var(C::KEY_MATERIAL_LEN);
        (0..n)
            .map(|i| {
                let mut hash = hash.clone();
                hash.update(&[i]);
                C::nonce_keypair_from_secret_bytes(hash.finalize_boxed().borrow())
            })
            .collect()
    }

    pub fn scalars_for_event_outcome(&self, stamped: &StampedOutcome) -> Vec<C::AttestScalar> {
        let event_id = &stamped.outcome.id;
        let event_seed = self.event_seed.child(event_id.as_bytes());
        let hash = event_seed.to_blake2b_var(C::KEY_MATERIAL_LEN);
        hash.clone().finalize_boxed();
        stamped
            .outcome
            .attestation_indexes()
            .iter()
            .enumerate()
            .map(|(i, index)| {
                let nonce_keypair = {
                    let mut hash = hash.clone();
                    hash.update(&[i as u8]);
                    C::nonce_keypair_from_secret_bytes(hash.finalize_boxed().borrow())
                };
                let scalar = C::reveal_attest_scalar(
                    &self.attestation_keypair,
                    nonce_keypair.clone(),
                    *index,
                );
                // Always verify the attestation before publishing it
                assert!(C::verify_attest_scalar(
                    &self.attestation_keypair.clone().into(),
                    &nonce_keypair.clone().into(),
                    *index,
                    &scalar
                ));
                scalar
            })
            .collect()
    }

    pub fn create_announcement(&self, event: Event) -> RawAnnouncement<C> {
        let nonces = self
            .nonces_for_event(&event.id)
            .into_iter()
            .map(|nonce_kp| nonce_kp.into())
            .collect::<Vec<_>>();
        RawAnnouncement::create(event, &self.announcement_keypair, nonces)
    }
}
