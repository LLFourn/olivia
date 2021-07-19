mod eq;
pub use eq::*;

use olivia_core::EventId;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum OutcomeFilter {
    All,
    Chosen(Vec<u64>),
}

impl OutcomeFilter {
    pub fn outcomes_for(&self, id: &EventId) -> Vec<u64> {
        match self {
            OutcomeFilter::All => (0..id.n_outcomes()).collect::<Vec<_>>(),
            OutcomeFilter::Chosen(chosen) => chosen.clone(),
        }
    }
}
