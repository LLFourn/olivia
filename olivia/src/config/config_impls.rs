use super::*;
use crate::{
    db::{self, postgres::PgBackendWrite, DbReadEvent, PrefixedDb},
    sources,
};
use olivia_core::{
    chrono, Event, EventId, EventKind, Node, NodeKind, Outcome, Path, PrefixPath, RangeKind,
    StampedOutcome, VsMatchKind,
};
use sources::{ticker::TimeOutcomeStream, Update};
use std::{fs, sync::Arc};
use tokio_stream as stream;
use tokio_stream::StreamMap;

impl Config {
    pub fn build_event_streams(
        &self,
        db: Arc<dyn DbReadEvent>,
        logger: slog::Logger,
    ) -> anyhow::Result<StreamMap<Path, sources::Stream<Event>>> {
        let mut streams = StreamMap::new();

        for (parent, source) in self.events.clone() {
            let db = PrefixedDb::new(db.clone(), parent.clone());
            let stream =
                source.to_event_stream(logger.new(o!("parent" => parent.to_string())), db)?;
            streams.insert(parent, stream);
        }

        Ok(streams)
    }

    pub fn build_outcome_streams(
        &self,
        db: Arc<dyn DbReadEvent>,
        secret_seed: &Seed,
        logger: slog::Logger,
    ) -> anyhow::Result<StreamMap<Path, sources::Stream<StampedOutcome>>> {
        let mut streams = StreamMap::new();

        for (parent, source) in self.outcomes.clone() {
            let db = PrefixedDb::new(db.clone(), parent.clone());
            let stream = source.to_outcome_stream(
                &secret_seed.child(parent.as_str().as_bytes()),
                logger.new(o!("parent" => parent.to_string())),
                db,
            )?;
            streams.insert(parent, stream);
        }

        Ok(streams)
    }

    pub fn build_node_streams(
        &self,
        logger: slog::Logger,
    ) -> anyhow::Result<StreamMap<Path, sources::Stream<Node>>> {
        let mut streams = StreamMap::new();
        for (parent, source) in self.events.clone() {
            let stream = source.to_node_stream(logger.new(o!("parent" => parent.to_string())))?;
            streams.insert(parent, stream);
        }
        Ok(streams)
    }
}

impl LoggerConfig {
    pub fn to_slog_drain(&self) -> anyhow::Result<RootDrain> {
        use crate::slog::Drain;
        use LoggerConfig::*;
        match &self {
            Term { out, color, level } => {
                let mut decorator = match out {
                    TermConfig::Stdout => slog_term::TermDecorator::new().stdout(),
                    TermConfig::Stderr => slog_term::TermDecorator::new().stderr(),
                };
                if let Some(color) = color {
                    decorator = if *color {
                        decorator.force_color()
                    } else {
                        decorator.force_plain()
                    }
                }
                let drain = slog_term::FullFormat::new(decorator.build())
                    .build()
                    .fuse()
                    .filter_level(*level)
                    .ignore_res();
                Ok(Box::new(
                    slog_async::Async::new(drain).chan_size(4096).build().fuse(),
                ))
            }
            File { path, level } => {
                let open_file = fs::OpenOptions::new()
                    .create(true)
                    .write(true)
                    .append(true)
                    .open(path)?;

                let decorator = slog_term::PlainDecorator::new(open_file);
                let drain = slog_term::FullFormat::new(decorator)
                    .build()
                    .fuse()
                    .filter_level(*level)
                    .ignore_res();
                Ok(Box::new(
                    slog_async::Async::new(drain).chan_size(4096).build().fuse(),
                ))
            }
        }
    }
}

impl LoggersConfig {
    pub fn to_slog_drain(&self) -> anyhow::Result<RootDrain> {
        let drains = self
            .0
            .iter()
            .map(|drain| drain.to_slog_drain())
            .collect::<Result<Vec<_>, _>>()?;

        // merge the drains into a single drain
        Ok(drains
            .into_iter()
            .fold(Box::new(slog::Discard) as RootDrain, |acc, drain| {
                Box::new(slog::IgnoreResult::new(slog::Duplicate::new(acc, drain)))
            }))
    }
}

impl EventSourceConfig {
    pub fn to_event_stream(
        &self,
        logger: slog::Logger,
        db: PrefixedDb,
    ) -> anyhow::Result<sources::Stream<Event>> {
        let config = self.clone();
        match config {
            EventSourceConfig::Redis(RedisConfig {
                connection_info,
                lists,
            }) => {
                info!(
                    logger,
                    "Connecting to redis://{}/{} to receive events",
                    connection_info.addr, connection_info.db, ;
                );

                Ok(Box::pin(sources::redis::event_stream(
                    redis::Client::open(connection_info.clone())?,
                    lists,
                    logger.new(o!("type" => "event_source", "source_type" => "redis")),
                )?))
            }
            EventSourceConfig::Ticker {
                look_ahead,
                interval,
                initial_time,
                ticker_kind,
            } => {
                let initial_time = initial_time.unwrap_or_else(|| {
                    use chrono::Timelike;
                    chrono::Utc::now()
                        .with_second(0)
                        .unwrap()
                        .with_nanosecond(0)
                        .unwrap()
                        .naive_utc()
                });

                Ok(Box::pin(
                    sources::ticker::TimeEventStream {
                        db,
                        look_ahead: chrono::Duration::seconds(look_ahead as i64),
                        interval: chrono::Duration::seconds(interval as i64),
                        initial_time,
                        logger: logger.new(o!("type" => "event_source", "source_type" => "ticker")),
                        event_creator: match ticker_kind {
                            TickerKind::Time => |time| EventId::occur_from_dt(time),
                            TickerKind::HeadsOrTails => |time| {
                                EventId::from_path_and_kind(
                                    Path::from_str("/heads_tails")
                                        .unwrap()
                                        .prefix_path(Path::from_dt(time).as_path_ref()),
                                    EventKind::VsMatch(VsMatchKind::Win),
                                )
                            },
                        },
                    }
                    .start(),
                ))
            }
            EventSourceConfig::Predicate {
                predicate: super::Predicate::Eq,
                on,
                over,
            } => {
                let mut inner = over.to_event_stream(logger, db)?;
                let pred = sources::predicates::Eq { outcome_filter: on };
                Ok(Box::pin(async_stream::stream! {
                    loop {
                        use tokio_stream::StreamExt;
                        match inner.next().await {
                            Some(update) => {
                                let pred_event_ids = pred.apply_to_event_id(&update.update.id);
                                let expected_outcome_time = update.update.expected_outcome_time;
                                yield update;
                                for id in pred_event_ids {
                                    yield Update::from(Event {
                                        id,
                                        expected_outcome_time
                                    });
                                }
                            }
                            _ => break,
                        }
                    }
                }))
            }
        }
    }

    pub fn to_node_stream(&self, _logger: slog::Logger) -> anyhow::Result<sources::Stream<Node>> {
        use EventSourceConfig::*;
        Ok(match *self {
            Ticker { interval, .. } => Box::pin(stream::iter(vec![Update {
                update: Node {
                    path: Path::root(),
                    kind: NodeKind::Range {
                        range_kind: RangeKind::Time { interval },
                    },
                },
                processed_notifier: None,
            }])),
            _ => Box::pin(stream::empty()),
        })
    }
}

impl OutcomeSourceConfig {
    pub fn to_outcome_stream(
        &self,
        seed: &Seed,
        logger: slog::Logger,
        db: PrefixedDb,
    ) -> anyhow::Result<sources::Stream<StampedOutcome>> {
        use OutcomeSourceConfig::*;
        match self.clone() {
            Redis(RedisConfig {
                connection_info,
                lists,
            }) => {
                info!(
                    logger,
                    "Connecting to redis://{}/{} to receive outcomes",
                    connection_info.addr, connection_info.db;
                );
                Ok(Box::pin(sources::redis::event_stream(
                    redis::Client::open(connection_info.clone())?,
                    lists,
                    logger.new(o!("source_type" => "redis")),
                )?))
            }
            Ticker { ticker_kind } => Ok(match ticker_kind {
                TickerKind::Time => Box::pin(
                    TimeOutcomeStream {
                        db,
                        logger: logger.new(o!("source_type" => "ticker", "ticker_kind" => "time")),
                        outcome_creator: |id| Outcome { id, value: 0 },
                    }
                    .start(),
                ),
                TickerKind::HeadsOrTails => {
                    let seed = seed.child(b"heads-or-tails-outcomes");
                    Box::pin(
                        TimeOutcomeStream {
                            db,
                            logger: logger
                                .new(o!("source_type" => "ticker", "ticker_kind" => "heads_tails")),
                            outcome_creator: move |id: EventId| {
                                let event_randomness = seed.child(id.as_bytes());
                                let value = (event_randomness.as_ref()[0] & 0x01) as u64;
                                Outcome { id, value }
                            },
                        }
                        .start(),
                    )
                }
            }),
            Predicate {
                predicate: super::Predicate::Eq,
                on,
                over,
            } => {
                let mut inner = over.to_outcome_stream(seed, logger, db)?;
                let pred = sources::predicates::Eq { outcome_filter: on };
                Ok(Box::pin(async_stream::stream! {
                    loop {
                        use tokio_stream::StreamExt;
                        match inner.next().await {
                            Some(update) => {
                                let pred_outcomes = pred.apply_to_outcome(&update.update.outcome);
                                let time = update.update.time;
                                yield update;
                                for pred_outcome in pred_outcomes {
                                    yield Update::from(StampedOutcome { outcome: pred_outcome, time } );
                                }
                            }
                            _ => break,
                        }
                    }
                }))
            }
        }
    }
}

lazy_static::lazy_static! {
    static ref IN_MEMORY: db::in_memory::InMemory<olivia_secp256k1::Secp256k1> = db::in_memory::InMemory::default();
}

impl DbConfig {
    pub async fn connect_database_read_group(
        &self,
    ) -> anyhow::Result<Arc<dyn db::DbReadOracle<olivia_secp256k1::Secp256k1>>> {
        match self {
            DbConfig::InMemory => Ok(Arc::new(IN_MEMORY.clone())),
            DbConfig::Postgres { url } => Ok(Arc::new(db::postgres::connect_read(url).await?)),
        }
    }

    pub async fn connect_database_read(&self) -> anyhow::Result<Arc<dyn db::DbReadEvent>> {
        match self {
            DbConfig::InMemory => Ok(Arc::new(IN_MEMORY.clone())),
            DbConfig::Postgres { url } => Ok(Arc::new(db::postgres::connect_read(url).await?)),
        }
    }

    pub async fn connect_database(
        &self,
    ) -> anyhow::Result<Arc<dyn db::Db<olivia_secp256k1::Secp256k1>>> {
        match self {
            DbConfig::InMemory => Ok(Arc::new(IN_MEMORY.clone())),
            DbConfig::Postgres { url } => Ok(Arc::new(PgBackendWrite::connect(url).await?)),
        }
    }
}
