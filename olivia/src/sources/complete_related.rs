use olivia_core::{EventKind, Outcome};

use crate::db::{DbReadEvent, EventQuery, PrefixedDb};

pub struct CompleteRelated {
    pub db: PrefixedDb,
}

impl CompleteRelated {
    pub async fn complete_related(&self, outcome: &Outcome) -> anyhow::Result<Vec<Outcome>> {
        if outcome.id.n_outcomes() < 3 {
            // things with less than 3 outcomes won't be depended on by anything.
            return Ok(vec![]);
        }
        let outcome_event_kind = outcome.id.event_kind();

        let related_events = self
            .db
            .query_events(EventQuery {
                // find sibling events of this event
                path: outcome.id.path().into(),
                ..Default::default()
            })
            .await?;

        Ok(related_events
            .into_iter()
            .filter_map(|related| match related.id.event_kind() {
                // If we have the outcome for the event we also have it for the predicated event.
                EventKind::Predicate { inner, predicate }
                    if inner.eq_fuzzy(&outcome_event_kind) =>
                {
                    let outcome_value = predicate.predicate_outcome(&outcome.outcome_string());
                    Some(Outcome {
                        id: related.id,
                        value: outcome_value,
                    })
                }
                // If we have a price outcome we don't care about nonces
                EventKind::Price { n_digits: _ }
                    if matches!(outcome_event_kind, EventKind::Price { n_digits: _ }) =>
                {
                    Some(Outcome {
                        id: related.id,
                        value: outcome.value,
                    })
                }
                _ => None,
            })
            .collect())
    }
}
