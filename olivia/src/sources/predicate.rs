use olivia_core::{EventId, PredicateKind};

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

#[derive(Clone, Debug)]
pub struct Predicate {
    pub outcome_filter: OutcomeFilter,
    pub predicate_kind: PredicateKind,
}

impl Predicate {
    pub fn apply_to_event_id(&self, id: &EventId) -> Vec<EventId> {
        self.outcome_filter
            .outcomes_for(id)
            .into_iter()
            .map(move |value| id.predicate(self.predicate_kind, value))
            .collect()
    }
}
