use crate::{
    seed::Seed,
    sources,
    sources::{
        subscriber::{self, Subscriber},
        Update,
    },
};
use futures::StreamExt;
use olivia_core::{
    Event, EventId, EventKind, Outcome, Path, PrefixPath, StampedOutcome, VsMatchKind,
};
use std::str::FromStr;

pub struct HeadsOrTailsEvents;

pub struct HeadsOrTailsOutcomes {
    pub seed: Seed,
}

impl Subscriber<Event> for HeadsOrTailsEvents {
    fn start(&self, events: subscriber::Stream<Event>) -> sources::Stream<Event> {
        events
            .filter(|event| {
                std::future::ready(event.id.event_kind() == EventKind::SingleOccurrence)
            })
            .map(|event| {
                Update::from(Event {
                    id: event_to_heads_tails(&event.id),
                    expected_outcome_time: event.expected_outcome_time,
                })
            })
            .boxed()
    }
}

fn event_to_heads_tails(id: &EventId) -> EventId {
    let path = Path::from_str("/heads_tails")
        .unwrap()
        .prefix_path(id.path());
    EventId::from_path_and_kind(path, EventKind::VsMatch(VsMatchKind::Win))
}

impl Subscriber<StampedOutcome> for HeadsOrTailsOutcomes {
    fn start(
        &self,
        outcomes: subscriber::Stream<StampedOutcome>,
    ) -> sources::Stream<StampedOutcome> {
        let seed = self.seed.clone();
        outcomes
            .filter(|stamped| {
                std::future::ready(stamped.outcome.id.event_kind() == EventKind::SingleOccurrence)
            })
            .map(move |stamped| {
                let event_id = event_to_heads_tails(&stamped.outcome.id);
                let event_randomness = seed.child(event_id.as_bytes());
                let value = (event_randomness.as_ref()[0] & 0x01) as u64;
                Update::from(StampedOutcome {
                    outcome: Outcome {
                        id: event_id,
                        value,
                    },
                    time: stamped.time,
                })
            })
            .boxed()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use olivia_core::EventId;
    use std::str::FromStr;

    #[tokio::test]
    async fn heads_vs_tails_remit_events() {
        let incoming: Vec<Event> = vec![
            EventId::from_str("/2020-09-30T08:00:00.occur")
                .unwrap()
                .into(),
            EventId::from_str("/2020-09-30T08:01:00.occur")
                .unwrap()
                .into(),
        ];

        let subscriber = HeadsOrTailsEvents;

        let mut outcoming = subscriber
            .start(futures::stream::iter(incoming).boxed())
            .map(|update| update.update.id.as_str().to_string())
            .collect::<Vec<String>>()
            .await;

        let mut expecting = vec![
            "/2020-09-30T08:00:00/heads_tails.win",
            "/2020-09-30T08:01:00/heads_tails.win",
        ];

        outcoming.sort();
        expecting.sort();

        assert_eq!(outcoming, expecting);
    }

    #[tokio::test]
    async fn heads_tails_remit_outcomes() {
        let time = chrono::Utc::now().naive_utc();
        let incoming: Vec<StampedOutcome> = vec![
            EventId::from_str("/2020-09-30T08:00:00.occur").unwrap(),
            EventId::from_str("/2020-09-30T08:01:00.occur").unwrap(),
            EventId::from_str("/2020-09-30T08:02:00.occur").unwrap(),
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

        let subscriber = HeadsOrTailsOutcomes {
            seed: Seed::new([42u8; 64]),
        };

        let mut outcoming = subscriber
            .start(futures::stream::iter(incoming).boxed())
            .map(|update| update.update.outcome.to_string())
            .collect::<Vec<String>>()
            .await;

        let mut expecting = vec![
            "/2020-09-30T08:00:00/heads_tails.win=tails_win",
            "/2020-09-30T08:01:00/heads_tails.win=heads_win",
            "/2020-09-30T08:02:00/heads_tails.win=heads_win",
        ];

        outcoming.sort();
        expecting.sort();

        assert_eq!(outcoming, expecting);
    }
}
