mod eq;
pub use eq::*;

use olivia_core::EventId;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum OutcomeFilter {
    Pattern(Pattern),
    Indexes(Vec<u64>),
}

impl OutcomeFilter {
    pub fn outcomes_for(&self, id: &EventId) -> Vec<u64> {
        match self {
            OutcomeFilter::Pattern(Pattern::All) => (0..id.n_outcomes()).collect::<Vec<_>>(),
            OutcomeFilter::Indexes(chosen) => chosen.clone(),
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum Pattern {
    #[serde(rename = "*")]
    All,
}
