use super::{EventReEmitter, OutcomeReEmitter};
use crate::sources::{EventStream, OutcomeStream, Update};
use futures::StreamExt;
use olivia_core::{Event, EventKind, Outcome, StampedOutcome, VsMatchKind};

pub struct Vs;

impl EventReEmitter for Vs {
    fn re_emit_events(&self, events: EventStream) -> EventStream {
        events
            .flat_map(|update| {
                let event = &update.update;
                let mut re_emit = vec![];

                if let EventKind::VsMatch(kind) = event.id.event_kind() {
                    if kind == VsMatchKind::WinOrDraw {
                        re_emit.push(Update::from(Event {
                            id: event.id.replace_kind(EventKind::VsMatch(VsMatchKind::Win)),
                            expected_outcome_time: event.expected_outcome_time,
                        }));
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
        outcomes
            .map(|update| {
                let stamped = &update.update;
                let id = &stamped.outcome.id;
                let mut re_emit = vec![];

                if let EventKind::VsMatch(VsMatchKind::WinOrDraw) = id.event_kind() {
                    let new_outcome = match stamped.outcome.value {
                        // A party won then pass through
                        x if x < 2 => x,
                        // If draw then the left did not win
                        _ => 1,
                    };
                    re_emit.push(Update::from(StampedOutcome {
                        time: stamped.time,
                        outcome: Outcome {
                            id: id.replace_kind(EventKind::VsMatch(VsMatchKind::Win)),
                            value: new_outcome,
                        },
                    }));
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
    use olivia_core::EventId;
    use std::str::FromStr;

    #[test]
    fn vs_re_emit_events() {
        let rt = tokio::runtime::Runtime::new().unwrap();

        let incoming: Vec<Update<Event>> = vec![
            EventId::from_str("/foo/bar/FOO_BAR.vs").unwrap().into(),
            EventId::from_str("/foo/baz/FOO_BAZ.vs").unwrap().into(),
        ];

        let re_emitter = Vs;

        let mut outcoming = rt.block_on(
            re_emitter
                .re_emit_events(futures::stream::iter(incoming).boxed())
                .map(|update| update.update.id.as_str().to_string())
                .collect::<Vec<String>>(),
        );

        let mut expecting = vec![
            "/foo/bar/FOO_BAR.win",
            "/foo/bar/FOO_BAR.vs",
            "/foo/baz/FOO_BAZ.win",
            "/foo/baz/FOO_BAZ.vs",
        ];

        expecting.sort();
        outcoming.sort();

        assert_eq!(outcoming, expecting);
    }

    #[test]
    fn vs_re_emit_outcomes() {
        use olivia_core::WinOrDraw;
        let rt = tokio::runtime::Runtime::new().unwrap();
        let time = chrono::Utc::now().naive_utc();
        let incoming: Vec<Update<StampedOutcome>> = {
            vec![
                StampedOutcome {
                    outcome: Outcome {
                        value: WinOrDraw::LeftWon as u64,
                        id: EventId::from_str("/foo/bar/FOO1_BAR1.vs").unwrap(),
                    },
                    time,
                },
                StampedOutcome {
                    outcome: Outcome {
                        value: WinOrDraw::RightWon as u64,
                        id: EventId::from_str("/foo/bar/FOO2_BAR2.vs").unwrap(),
                    },
                    time,
                },
                StampedOutcome {
                    outcome: Outcome {
                        id: EventId::from_str("/foo/bar/FOO3_BAR3.vs").unwrap(),
                        value: WinOrDraw::Draw as u64,
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
            "/foo/bar/FOO1_BAR1.vs=FOO1_win",
            "/foo/bar/FOO1_BAR1.win=FOO1_win",
            "/foo/bar/FOO2_BAR2.vs=BAR2_win",
            "/foo/bar/FOO2_BAR2.win=BAR2_win",
            "/foo/bar/FOO3_BAR3.vs=draw",
            "/foo/bar/FOO3_BAR3.win=BAR3_win",
        ];
        outcoming.sort();
        expecting.sort();

        assert_eq!(outcoming, expecting)
    }
}
