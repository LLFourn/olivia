use super::OutcomeFilter;
use olivia_core::{EventId, Outcome};

pub struct Eq {
    pub outcome_filter: OutcomeFilter,
}

impl Eq {
    pub fn apply_to_event_id(&self, id: &EventId) -> Vec<EventId> {
        self.outcome_filter
            .outcomes_for(id)
            .into_iter()
            .map(move |value| id.predicate_eq(value))
            .collect()
    }

    pub fn apply_to_outcome(&self, actual_outcome: &Outcome) -> Vec<Outcome> {
        self.outcome_filter
            .outcomes_for(&actual_outcome.id)
            .into_iter()
            .map(|value| actual_outcome.predicate_eq(value))
            .collect()
    }
}
