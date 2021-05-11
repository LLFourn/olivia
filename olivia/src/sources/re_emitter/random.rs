use super::{EventReEmitter, OutcomeReEmitter};
use crate::{seed::Seed, sources::Update};
use futures::StreamExt;
use olivia_core::{Event, EventId, EventKind, Outcome, StampedOutcome};
use std::str::FromStr;

pub struct HeadsOrTailsEvents;

pub struct HeadsOrTailsOutcomes {
    pub seed: Seed,
}

impl EventReEmitter for HeadsOrTailsEvents {
    fn re_emit_events(&self, events: crate::sources::EventStream) -> crate::sources::EventStream {
        events
            .flat_map(|update| {
                let event = &update.update;
                let mut re_emit = vec![];
                if let Some(event_id) = time_event_to_random(&event.id) {
                    re_emit.push(Update::from(Event {
                        id: event_id,
                        expected_outcome_time: event.expected_outcome_time,
                    }))
                }
                re_emit.push(update);
                futures::stream::iter(re_emit)
            })
            .boxed()
    }
}

fn time_event_to_random(id: &EventId) -> Option<EventId> {
    if EventKind::SingleOccurrence == id.event_kind() && id.as_path().segment(0) == Some("time") {
        let time = id.as_path().segment(1).unwrap();
        match EventId::from_str(&format!("/random/{}/heads_tails?win", time)) {
            Ok(new_id) => Some(new_id),
            Err(_) => None,
        }
    } else {
        None
    }
}

impl OutcomeReEmitter for HeadsOrTailsOutcomes {
    fn re_emit_outcomes(
        &self,
        outcomes: crate::sources::OutcomeStream,
    ) -> crate::sources::OutcomeStream {
        let seed = self.seed.clone();
        outcomes
            .flat_map(move |update| {
                let stamped = &update.update;
                let mut re_emit = vec![];
                if let EventKind::SingleOccurrence = stamped.outcome.id.event_kind() {
                    if let Some(event_id) = time_event_to_random(&stamped.outcome.id) {
                        let event_randomness = seed.child(event_id.as_bytes());
                        let value = (event_randomness.as_ref()[0] & 0x01) as u64;
                        re_emit.push(Update::from(StampedOutcome {
                            outcome: Outcome {
                                id: event_id,
                                value,
                            },
                            time: stamped.time,
                        }))
                    }
                }
                re_emit.push(update);
                futures::stream::iter(re_emit)
            })
            .boxed()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::core::EventId;
    use std::str::FromStr;

    #[tokio::test]
    async fn heads_vs_tails_remit_events() {
        let incoming: Vec<Update<Event>> = vec![
            EventId::from_str("/time/2020-09-30T08:00:00?occur")
                .unwrap()
                .into(),
            EventId::from_str("/time/2020-09-30T08:01:00?occur")
                .unwrap()
                .into(),
        ];

        let re_emitter = HeadsOrTailsEvents;

        let mut outcoming = re_emitter
            .re_emit_events(futures::stream::iter(incoming).boxed())
            .map(|update| update.update.id.as_str().to_string())
            .collect::<Vec<String>>()
            .await;

        let mut expecting = vec![
            "/time/2020-09-30T08:00:00?occur",
            "/time/2020-09-30T08:01:00?occur",
            "/random/2020-09-30T08:00:00/heads_tails?win",
            "/random/2020-09-30T08:01:00/heads_tails?win",
        ];

        outcoming.sort();
        expecting.sort();

        assert_eq!(outcoming, expecting);
    }

    #[tokio::test]
    async fn heads_tails_remit_outcomes() {
        let time = chrono::Utc::now().naive_utc();
        let incoming: Vec<Update<StampedOutcome>> = vec![
            EventId::from_str("/time/2020-09-30T08:00:00?occur").unwrap(),
            EventId::from_str("/time/2020-09-30T08:01:00?occur").unwrap(),
        ]
        .into_iter()
        .map(|id| {
            StampedOutcome {
                outcome: Outcome {
                    value: olivia_core::Occur::Occurred as u64,
                    id,
                },
                time,
            }
            .into()
        })
        .collect();

        let re_emitter = HeadsOrTailsOutcomes {
            seed: Seed::new([42u8; 64]),
        };

        let mut outcoming = re_emitter
            .re_emit_outcomes(futures::stream::iter(incoming).boxed())
            .map(|update| update.update.outcome.to_string())
            .collect::<Vec<String>>()
            .await;

        let mut expecting = vec![
            "/time/2020-09-30T08:00:00?occur=true",
            "/time/2020-09-30T08:01:00?occur=true",
            "/random/2020-09-30T08:00:00/heads_tails?win=tails_win",
            "/random/2020-09-30T08:01:00/heads_tails?win=heads_win",
        ];

        outcoming.sort();
        expecting.sort();

        assert_eq!(outcoming, expecting);
    }
}
