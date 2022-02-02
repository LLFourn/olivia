use super::*;
use crate::{
    db::{self, postgres::PgBackendWrite, DbReadEvent, PrefixedDb},
    sources::{
        self,
        ticker::{RandomOutcomeCreator, ZeroOutcomeCreator},
    },
};
use olivia_core::{chrono, Event, Node, NodeKind, Path, RangeKind, StampedOutcome};
use sources::{ticker::TimeOutcomeStream, Update};
use std::{fs, sync::Arc};
use tokio_stream as stream;
use tokio_stream::StreamMap;

impl Config {
    pub fn build_event_streams(
        &self,
        db: Arc<dyn DbReadEvent>,
        logger: slog::Logger,
    ) -> anyhow::Result<StreamMap<(Path, usize), sources::Stream<Event>>> {
        let mut streams = StreamMap::new();

        for (parent, sources) in self.events.clone() {
            let db = PrefixedDb::new(db.clone(), parent.clone());
            let logger = logger.new(o!("path" => parent.to_string()));
            for (i, source) in sources.into_iter().enumerate() {
                let stream = source.to_event_stream(logger.clone(), db.clone())?;
                streams.insert((parent.clone(), i), stream);
            }
        }

        Ok(streams)
    }

    pub fn build_outcome_streams(
        &self,
        db: Arc<dyn DbReadEvent>,
        secret_seed: &Seed,
        logger: slog::Logger,
    ) -> anyhow::Result<StreamMap<(Path, usize), sources::Stream<StampedOutcome>>> {
        let mut streams = StreamMap::new();

        for (parent, sources) in self.outcomes.clone() {
            let db = PrefixedDb::new(db.clone(), parent.clone());
            let logger = logger.new(o!("path" => parent.to_string()));
            for (i, source) in sources.into_iter().enumerate() {
                let stream = source.to_outcome_stream(
                    secret_seed.child(parent.as_str().as_bytes()),
                    logger.clone(),
                    db.clone(),
                )?;
                streams.insert((parent.clone(), i), stream);
            }
        }

        Ok(streams)
    }

    pub fn build_node_streams(
        &self,
        logger: slog::Logger,
    ) -> anyhow::Result<StreamMap<(Path, usize), sources::Stream<Node>>> {
        let mut streams = StreamMap::new();
        for (parent, sources) in self.events.clone() {
            for (i, source) in sources.iter().enumerate() {
                let stream = source.to_node_stream(logger.new(o!("path" => parent.to_string())))?;
                streams.insert((parent.clone(), i), stream);
            }
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
        let mut stream: sources::Stream<Event> = match config.event_source {
            EventSource::Redis(RedisConfig {
                connection_info,
                lists,
            }) => {
                info!(
                    logger,
                    "Connecting to redis://{} to receive events",
                    connection_info.addr;
                );

                let connection = redis::Client::open(connection_info.clone())?;
                info!(
                    logger,
                    "succesfully connected to redis://{}", connection_info.addr
                );

                Box::pin(sources::redis::event_stream(
                    connection,
                    lists,
                    logger.new(o!("type" => "event_source", "source_type" => "redis")),
                )?)
            }
            EventSource::Ticker {
                look_ahead,
                interval,
                initial_time,
                ends_with,
                event_kind,
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

                let logger = logger.new(o!("type" => "event_source", "source_type" => "ticker"));
                let look_ahead = chrono::Duration::seconds(look_ahead as i64);
                let interval = chrono::Duration::seconds(interval as i64);

                Box::pin(
                    sources::ticker::TimeEventStream {
                        db,
                        look_ahead,
                        interval,
                        initial_time,
                        logger,
                        ends_with,
                        event_kind,
                    }
                    .start(),
                )
            }
        };

        if let Some(predicate) = self.predicate.clone() {
            match predicate {
                PredicateConfig { kind, filter } => {
                    let pred = sources::predicate::Predicate {
                        outcome_filter: filter,
                        predicate_kind: kind.into(),
                    };
                    Ok(Box::pin(async_stream::stream! {
                        loop {
                            use tokio_stream::StreamExt;
                            match stream.next().await {
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
        } else {
            Ok(stream)
        }
    }

    pub fn to_node_stream(&self, _logger: slog::Logger) -> anyhow::Result<sources::Stream<Node>> {
        use EventSource::*;
        Ok(match self.event_source {
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
        seed: Seed,
        logger: slog::Logger,
        db: PrefixedDb,
    ) -> anyhow::Result<sources::Stream<StampedOutcome>> {
        use OutcomeSource::*;
        info!(logger, "starting outcome stream"; "config" => serde_json::to_string(&self).unwrap());
        let mut stream: sources::Stream<StampedOutcome> = match self.clone().outcome_source {
            Redis(RedisConfig {
                connection_info,
                lists,
            }) => {
                info!(
                    logger,
                    "Connecting to redis://{} to receive outcomes on {}",
                    connection_info.addr, lists.join(",");
                );
                let conn = redis::Client::open(connection_info.clone())?;
                info!(
                    logger,
                    "succesfully connected to redis://{}", connection_info.addr
                );
                Box::pin(sources::redis::event_stream(
                    conn,
                    lists,
                    logger.new(o!("source_type" => "redis")),
                )?)
            }
            Random {
                ends_with,
                event_kind,
                max,
            } => Box::pin(
                TimeOutcomeStream {
                    db: db.clone(),
                    logger: logger.new(o!("source_type" => "random")),
                    ends_with,
                    event_kind,
                    outcome_creator: RandomOutcomeCreator { seed, max },
                }
                .start(),
            ),
            Zero {
                ends_with,
                event_kind,
            } => Box::pin(
                TimeOutcomeStream {
                    db: db.clone(),
                    logger: logger.new(o!("source_type" => "zero")),
                    ends_with,
                    event_kind,
                    outcome_creator: ZeroOutcomeCreator,
                }
                .start(),
            ),
        };

        if self.complete_related {
            debug!(logger, "complete related enabled");
            Ok(Box::pin(async_stream::stream! {
                let complete_related = sources::complete_related::CompleteRelated { db };
                let logger = logger.new(o!("source_type" => "complete_related"));
                loop {
                    use tokio_stream::StreamExt;
                    match stream.next().await {
                        Some(update) => {
                            let stamped_outcome = update.update.clone();
                            yield update;

                            match complete_related.complete_related(&stamped_outcome.outcome).await {
                                Ok(related_outcomes) => for outcome in related_outcomes {
                                    yield Update::from(StampedOutcome { outcome, time: stamped_outcome.time } );
                                },
                                Err(e) => error!(logger, "completing related";
                                                 "id" => stamped_outcome.outcome.id.as_str(),
                                                 "error" => e.to_string())
                            }
                        }
                        _ => break,
                    }
                }
            }))
        } else {
            debug!(logger, "complete related disabled");
            Ok(stream)
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

impl From<PredicateKind> for olivia_core::PredicateKind {
    fn from(from: PredicateKind) -> Self {
        match from {
            PredicateKind::Eq => olivia_core::PredicateKind::Eq,
            PredicateKind::Gt => olivia_core::PredicateKind::Bound(olivia_core::BoundKind::Gt),
        }
    }
}
