use blake2::{digest::Digest, Blake2b, VarBlake2b};

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
        blake2::crypto_mac::NewMac::new((&self.0).into())
    }

    pub fn to_blake2b_32(&self) -> VarBlake2b {
        VarBlake2b::new_keyed(&self.0, 32)
    }

    pub fn child(&self, tag: &[u8]) -> Self {
        Seed(self.to_blake2b().chain(tag).finalize().into())
    }

    pub const fn new(bytes: [u8; 64]) -> Self {
        Self(bytes)
    }
}

impl AsRef<[u8; 64]> for Seed {
    fn as_ref(&self) -> &[u8; 64] {
        &self.0
    }
}

#[cfg(test)]
impl Default for Seed {
    fn default() -> Self {
        Seed::new(*b"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx")
    }
}
