#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Descriptor {
    Enum {
        outcomes: Vec<String>,
    },
    DigitDecomposition {
        is_signed: bool,
        n_digits: u8,
        unit: Option<String>,
    },
    /// If the DLC spec doesn't support this
    MissingDescriptor,
}
