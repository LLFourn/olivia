use crate::{
    db::{DbReadEvent, EventQuery, Order, PrefixedDb},
    seed::Seed,
    sources::Update,
};
use olivia_core::{
    chrono,
    chrono::{Duration, NaiveDateTime},
    Event, EventId, EventKind, Outcome, Path, PathRef, StampedOutcome, VsMatchKind,
};
use tokio::{sync::oneshot, time};
use tokio_stream as stream;

pub struct TimeEventStream<F> {
    pub db: PrefixedDb,
    pub look_ahead: Duration,
    pub interval: Duration,
    pub initial_time: NaiveDateTime,
    pub logger: slog::Logger,
    pub event_creator: F,
}

impl<F> TimeEventStream<F>
where
    F: EventCreator + 'static + Send + Sync,
{
    pub fn start(self) -> impl stream::Stream<Item = Update<Event>> {
        let TimeEventStream {
            db,
            look_ahead,
            interval,
            initial_time,
            logger,
            event_creator,
        } = self;

        async_stream::stream! {
        let create_update = |dt| {
            let (sender, receiver) = oneshot::channel();
            (
                Update {
                    update: Event {
                        id: event_creator.create_event_id(dt),
                        expected_outcome_time: Some(dt),
                    },
                    processed_notifier: Some(sender),
                },
                receiver,
            )
        };

        loop  {
            let (ends_with, kind) = event_creator.event_filters();
            let latest = db.query_event(EventQuery {
                ends_with,
                kind,
                order: Order::Latest,
                ..Default::default()
            }).await;
            let (update, waiting) = match latest {
                Ok(Some(latest)) => {
                    let latest = latest
                        .expected_outcome_time
                        .expect("time events always have this");
                    // If the latest event we have in the DB is 19:36 and our interval is 1min
                    // then the next event we want is 19:37.
                    let next_event = latest + interval;
                    // But we should add it at 18:36 if our look_ahead is 1hr
                    let add_when = next_event - look_ahead;
                    // wait until then before returning it
                    delay_until(add_when).await;
                    create_update(next_event)
                }
                Ok(None) => {
                    // This means this is our first run against this backend, we add a new event to get us started.
                    create_update(initial_time)
                }
                Err(err) => {
                    crit!(
                        logger,
                        "Stopping emitting new time events as we got a DB error";
                        "error" => err.to_string()
                    );
                    break;
                }
            };
            let event_id = update.update.id.clone();

            yield update;

            if let Err(_) | Ok(true) = waiting.await {
                error!(logger, "processing of new ticker failed (will try again)"; "id" => event_id.as_str());
                time::sleep(std::time::Duration::from_secs(10)).await;
            }
        }
            }
    }
}

pub struct TimeOutcomeStream<F> {
    pub db: PrefixedDb,
    pub logger: slog::Logger,
    pub outcome_creator: F,
}

impl<F> TimeOutcomeStream<F>
where
    F: OutcomeCreator,
{
    pub fn start(self) -> impl stream::Stream<Item = Update<StampedOutcome>> {
        let TimeOutcomeStream {
            db,
            logger,
            outcome_creator,
        } = self;
        async_stream::stream! {
            loop {
                let (ends_with, kind) = outcome_creator.event_filters();
                let event = db.query_event(EventQuery {
                    attested: Some(false),
                    order: Order::Earliest,
                    ends_with,
                    kind,
                    ..Default::default()
                }).await;
                let event = match event {
                    Ok(Some(event)) => event,
                    Err(e) => {
                        crit!(
                            logger,
                            "DB error during outcome stream";
                            "error" => format!("{}", e)
                        );
                        time::sleep(std::time::Duration::from_secs(60)).await;
                        continue;
                    }
                    Ok(None) => {
                        time::sleep(std::time::Duration::from_secs(1)).await;
                        continue;
                    }
                };

                let event_complete_time = event
                    .expected_outcome_time
                    .expect("time events always have this");

                delay_until(event_complete_time).await;

                let (sender, waiting) = oneshot::channel();

                yield Update {
                    update: StampedOutcome {
                        outcome: outcome_creator.create_outcome(event.id.clone()),
                        time: now(), // tell the actual truth about when we actually figured it was done
                    },
                    processed_notifier: Some(sender),
                };

                if let Err(_) | Ok(true) = waiting.await {
                    error!(logger, "processing of ticker outcome failed (will try again)"; "id" => event.id.as_str());
                    time::sleep(std::time::Duration::from_secs(10)).await;
                }
            }
        }
    }
}

async fn delay_until(until: NaiveDateTime) {
    let delta = until - now();
    if delta > Duration::zero() {
        time::sleep(delta.to_std().unwrap().into()).await;
    }
}

fn now() -> NaiveDateTime {
    chrono::Utc::now().naive_utc()
}

pub trait EventFilter {
    fn event_filters(&self) -> (Option<PathRef<'_>>, Option<EventKind>);
}

pub trait EventCreator: EventFilter {
    fn create_event_id(&self, dt: NaiveDateTime) -> EventId;
}

pub trait OutcomeCreator: EventFilter {
    fn create_outcome(&self, id: EventId) -> Outcome;
}

pub struct Time;
pub struct HeadsOrTailsEvents;
pub struct HeadsOrTailsOutcomes {
    pub seed: Seed,
}

static HEADS_TAIL_PATH: PathRef = PathRef::from_str_unchecked("/heads_tails");

impl EventFilter for Time {
    fn event_filters(&self) -> (Option<PathRef<'_>>, Option<EventKind>) {
        (None, Some(EventKind::SingleOccurrence))
    }
}

impl EventFilter for HeadsOrTailsEvents {
    fn event_filters(&self) -> (Option<PathRef<'_>>, Option<EventKind>) {
        (
            Some(HEADS_TAIL_PATH),
            Some(EventKind::VsMatch(VsMatchKind::Win)),
        )
    }
}

impl EventFilter for HeadsOrTailsOutcomes {
    fn event_filters(&self) -> (Option<PathRef<'_>>, Option<EventKind>) {
        HeadsOrTailsEvents.event_filters()
    }
}

impl EventCreator for Time {
    fn create_event_id(&self, dt: NaiveDateTime) -> EventId {
        EventId::occur_from_dt(dt)
    }
}

impl OutcomeCreator for Time {
    fn create_outcome(&self, id: EventId) -> Outcome {
        Outcome { id, value: 0 }
    }
}

impl EventCreator for HeadsOrTailsEvents {
    fn create_event_id(&self, dt: NaiveDateTime) -> EventId {
        use olivia_core::PrefixPath;
        EventId::from_path_and_kind(
            HEADS_TAIL_PATH
                .to_path()
                .prefix_path(Path::from_dt(dt).as_path_ref()),
            EventKind::VsMatch(VsMatchKind::Win),
        )
    }
}

impl OutcomeCreator for HeadsOrTailsOutcomes {
    fn create_outcome(&self, id: EventId) -> Outcome {
        let event_randomness = self.seed.child(id.as_bytes());
        let value = (event_randomness.as_ref()[0] & 0x01) as u64;
        Outcome { id, value }
    }
}
