use blake2::{crypto_mac::NewMac, Blake2b, VarBlake2b};
use digest::generic_array::{typenum::U64, GenericArray};

#[derive(Clone)]
pub struct Seed([u8; 64]);

olivia_core::impl_fromstr_deserailize! {
    name => "oracle seed",
    fn from_bytes(bytes: [u8;64]) -> Option<Seed> {
        Some(Seed(bytes))
    }
}

impl std::fmt::Debug for Seed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", "XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX")
    }
}

impl Seed {
    pub fn to_blake2b(&self) -> Blake2b {
        Blake2b::new(GenericArray::<u8, U64>::from_slice(&self.0[..]))
    }

    pub fn to_blake2b_32(&self) -> VarBlake2b {
        VarBlake2b::new_keyed(&self.0, 32)
    }

    pub fn child(&self, tag: &[u8]) -> Self {
        let mut hash = self.to_blake2b();
        digest::Digest::update(&mut hash, tag);
        let mut result = [0u8; 64];
        result.copy_from_slice(&digest::Digest::finalize(hash));
        Seed(result)
    }

    pub const fn new(bytes: [u8; 64]) -> Self {
        Self(bytes)
    }
}

#[cfg(test)]
impl Default for Seed {
    fn default() -> Self {
        Seed::new(*b"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx")
    }
}
