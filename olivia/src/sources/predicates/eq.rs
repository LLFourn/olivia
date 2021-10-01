use super::OutcomeFilter;
use olivia_core::EventId;

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
}
