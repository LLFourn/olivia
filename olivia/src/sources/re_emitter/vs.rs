use super::{EventReEmitter, OutcomeReEmitter};
use crate::{
    core::{Event, EventKind, OutcomeValue, StampedOutcome, VsMatchKind, VsOutcome},
    sources::{EventStream, OutcomeStream, Update},
};
use futures::StreamExt;
use olivia_core::Outcome;

pub struct Vs;

impl EventReEmitter for Vs {
    fn re_emit_events(&self, events: EventStream) -> EventStream {
        events
            .flat_map(|update| {
                let event = &update.update;
                let mut re_emit = vec![];

                if let EventKind::VsMatch(kind) = event.id.event_kind() {
                    if kind == VsMatchKind::WinOrDraw {
                        for &right_posited_to_win in &[true, false] {
                            re_emit.push(Update::from(Event {
                                id: event.id.replace_kind(EventKind::VsMatch(VsMatchKind::Win {
                                    right_posited_to_win,
                                })),
                                expected_outcome_time: event.expected_outcome_time,
                            }));
                        }
                    }
                }

                re_emit.push(update);
                futures::stream::iter(re_emit)
            })
            .boxed()
    }
}

impl OutcomeReEmitter for Vs {
    fn re_emit_outcomes(&self, outcomes: OutcomeStream) -> OutcomeStream {
        use OutcomeValue::*;
        use VsOutcome::*;
        outcomes
            .map(|update| {
                let stamped = &update.update;
                let id = &stamped.outcome.id;
                let mut re_emit = vec![];

                if let Vs(ref vs_outcome) = stamped.outcome.value {
                    let (left, right) = id.parties().unwrap();
                    for &right_posited_to_win in &[true, false] {
                        let new_outcome = match vs_outcome {
                            Winner(winner) => OutcomeValue::Win {
                                winning_side: winner.clone(),
                                posited_won: right_posited_to_win == (right == winner),
                            },
                            Draw => OutcomeValue::Win {
                                winning_side: if right_posited_to_win {
                                    left.to_string()
                                } else {
                                    right.to_string()
                                },
                                posited_won: false,
                            },
                        };
                        re_emit.push(Update::from(StampedOutcome {
                            time: stamped.time,
                            outcome: Outcome {
                                id: id.replace_kind(EventKind::VsMatch(VsMatchKind::Win {
                                    right_posited_to_win,
                                })),
                                value: new_outcome,
                            },
                        }));
                    }
                }

                re_emit.push(update);
                futures::stream::iter(re_emit)
            })
            .flatten()
            .boxed()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::core::EventId;
    use std::str::FromStr;

    #[test]
    fn vs_re_emit_events() {
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        let incoming: Vec<Update<Event>> = vec![
            EventId::from_str("/foo/bar/FOO_BAR?vs").unwrap().into(),
            EventId::from_str("/foo/baz/FOO_BAZ?vs").unwrap().into(),
        ];

        let re_emitter = Vs;

        let mut outcoming = rt.block_on(
            re_emitter
                .re_emit_events(futures::stream::iter(incoming).boxed())
                .map(|update| update.update.id.as_str().to_string())
                .collect::<Vec<String>>(),
        );

        let mut expecting = vec![
            "/foo/bar/FOO_BAR?left-win",
            "/foo/bar/FOO_BAR?right-win",
            "/foo/bar/FOO_BAR?vs",
            "/foo/baz/FOO_BAZ?left-win",
            "/foo/baz/FOO_BAZ?right-win",
            "/foo/baz/FOO_BAZ?vs",
        ];

        expecting.sort();
        outcoming.sort();

        assert_eq!(outcoming, expecting);
    }

    #[test]
    fn vs_re_emit_outcomes() {
        let mut rt = tokio::runtime::Runtime::new().unwrap();
        let time = chrono::Utc::now().naive_utc();
        let incoming: Vec<Update<StampedOutcome>> = {
            use OutcomeValue::*;
            use VsOutcome::*;
            vec![
                StampedOutcome {
                    outcome: Outcome {
                        value: Vs(Winner("FOO1".to_string())),
                        id: EventId::from_str("/foo/bar/FOO1_BAR1?vs").unwrap(),
                    },
                    time,
                },
                StampedOutcome {
                    outcome: Outcome {
                        value: Vs(Winner("BAR2".to_string())),
                        id: EventId::from_str("/foo/bar/FOO2_BAR2?vs").unwrap(),
                    },
                    time,
                },
                StampedOutcome {
                    outcome: Outcome {
                        id: EventId::from_str("/foo/bar/FOO3_BAR3?vs").unwrap(),
                        value: Vs(Draw),
                    },
                    time,
                },
            ]
            .into_iter()
            .map(Update::from)
            .collect()
        };

        let re_emitter = Vs;

        let mut outcoming = rt.block_on(
            re_emitter
                .re_emit_outcomes(futures::stream::iter(incoming).boxed())
                .map(|update| update.update.outcome.to_string())
                .collect::<Vec<String>>(),
        );

        let mut expecting = vec![
            "/foo/bar/FOO1_BAR1?vs=FOO1_win",
            "/foo/bar/FOO1_BAR1?left-win=FOO1_win",
            "/foo/bar/FOO1_BAR1?right-win=FOO1_win",
            "/foo/bar/FOO2_BAR2?vs=BAR2_win",
            "/foo/bar/FOO2_BAR2?left-win=BAR2_win",
            "/foo/bar/FOO2_BAR2?right-win=BAR2_win",
            "/foo/bar/FOO3_BAR3?vs=draw",
            "/foo/bar/FOO3_BAR3?left-win=BAR3_win",
            "/foo/bar/FOO3_BAR3?right-win=FOO3_win",
        ];
        outcoming.sort();
        expecting.sort();

        assert_eq!(outcoming, expecting)
    }
}
