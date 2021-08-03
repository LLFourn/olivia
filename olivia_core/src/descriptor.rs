#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Descriptor {
    Enum {
        outcomes: Vec<String>,
    },
    DigitDecomposition {
        base: usize,
        is_signed: bool,
        n_digits: u8,
        unit: Option<String>,
    },
}

impl Descriptor {
    pub fn n_nonces(&self) -> usize {
        use Descriptor::*;
        match self {
            Enum { .. } => 1,
            DigitDecomposition {
                n_digits,
                is_signed,
                ..
            } => (*n_digits as usize) + (*is_signed as usize),
        }
    }
}
