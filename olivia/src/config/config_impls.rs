use super::*;
use crate::{
    broadcaster::Broadcaster,
    db::{self, postgres::PgBackendWrite, DbReadEvent, PrefixedDb},
    sources,
};
use futures::StreamExt;
use olivia_core::{Event, Node, NodeKind, Path, PrefixPath, RangeKind, StampedOutcome};
use sources::{time_ticker::TimeOutcomeStream, Update};
use std::{fs, sync::Arc};

impl Config {
    pub fn build_event_streams(
        &self,
        db: Arc<dyn DbReadEvent>,
        logger: slog::Logger,
        broadcaster: &mut Broadcaster<Event>,
    ) -> anyhow::Result<Vec<sources::Stream<Event>>> {
        let mut streams = vec![];

        for (parent, source) in self.events.clone() {
            let db = PrefixedDb::new(db.clone(), parent.clone());
            let stream = source.to_event_stream(
                logger.new(o!("parent" => parent.to_string())),
                db,
                broadcaster,
            )?;
            let prefixed_stream =
                stream.map(move |update| update.prefix_path(parent.as_path_ref()));
            streams.push(prefixed_stream.boxed())
        }

        Ok(streams)
    }

    pub fn build_outcome_streams(
        &self,
        db: Arc<dyn DbReadEvent>,
        secret_seed: &Seed,
        logger: slog::Logger,
        broadcaster: &mut Broadcaster<StampedOutcome>,
    ) -> anyhow::Result<Vec<sources::Stream<StampedOutcome>>> {
        let mut streams = vec![];

        for (parent, source) in self.outcomes.clone() {
            let db = PrefixedDb::new(db.clone(), parent.clone());
            let stream = source.to_outcome_stream(
                &secret_seed.child(parent.as_str().as_bytes()),
                logger.new(o!("parent" => parent.to_string())),
                db,
                broadcaster,
            )?;
            let prefixed_stream =
                stream.map(move |update| update.prefix_path(parent.as_path_ref()));
            streams.push(prefixed_stream.boxed());
        }

        Ok(streams)
    }

    pub fn build_node_streams(
        &self,
        logger: slog::Logger,
        broadcaster: &mut Broadcaster<Node>,
    ) -> anyhow::Result<Vec<sources::Stream<Node>>> {
        let mut streams = vec![];
        for (parent, source) in self.events.clone() {
            let stream = source
                .to_node_stream(logger.new(o!("parent" => parent.to_string())), broadcaster)?;
            let prefixed_stream =
                stream.map(move |update| update.prefix_path(parent.as_path_ref()));
            streams.push(prefixed_stream.boxed());
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
        broadcaster: &mut Broadcaster<Event>,
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

                Ok(sources::redis::event_stream(
                    redis::Client::open(connection_info.clone())?,
                    lists,
                    logger.new(o!("type" => "event_source", "source_type" => "redis")),
                )?
                .boxed())
            }
            EventSourceConfig::TimeTicker {
                look_ahead,
                interval,
                initial_time,
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

                Ok(sources::time_ticker::TimeEventStream {
                    db,
                    look_ahead: chrono::Duration::seconds(look_ahead as i64),
                    interval: chrono::Duration::seconds(interval as i64),
                    initial_time,
                    logger: logger
                        .new(o!("type" => "event_source", "source_type" => "time_ticker")),
                }
                .start()
                .boxed())
            }
            EventSourceConfig::Subscriber {
                subscribe,
                subscriber,
            } => {
                let subscriber = subscriber.to_subscriber();
                Ok(subscriber
                    .start(broadcaster.subscribe_to(subscribe).boxed())
                    .boxed())
            }
        }
    }

    pub fn to_node_stream(
        &self,
        _logger: slog::Logger,
        broadcaster: &mut Broadcaster<Node>,
    ) -> anyhow::Result<sources::Stream<Node>> {
        use EventSourceConfig::*;
        Ok(match *self {
            TimeTicker { interval, .. } => futures::stream::iter(vec![Update {
                update: Node {
                    path: Path::root(),
                    kind: NodeKind::Range {
                        range_kind: RangeKind::Time { interval },
                    },
                },
                processed_notifier: None,
            }])
            .boxed(),
            Subscriber {
                ref subscribe,
                subscriber: EventSubscriberConfig::HeadsOrTails,
            } => {
                // simply re-emit the node changes of the namespace we are subscribed to
                broadcaster
                    .subscribe_to(subscribe.clone())
                    .map(Update::new)
                    .boxed()
            }
            _ => futures::stream::empty().boxed(),
        })
    }
}

impl OutcomeSourceConfig {
    pub fn to_outcome_stream(
        &self,
        seed: &Seed,
        logger: slog::Logger,
        db: PrefixedDb,
        broadcaster: &mut Broadcaster<StampedOutcome>,
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
                Ok(sources::redis::event_stream(
                    redis::Client::open(connection_info.clone())?,
                    lists,
                    logger.new(o!("type" => "outcome_source", "source_type" => "redis")),
                )?
                .boxed())
            }
            TimeTicker {} => Ok(TimeOutcomeStream {
                db,
                logger: logger.new(o!("type" => "outcome_source", "source_type" => "time_ticker")),
            }
            .start()
            .boxed()),
            Subscriber {
                subscribe,
                subscriber,
            } => {
                let subscriber = subscriber.to_subscriber(seed);
                Ok(subscriber
                    .start(broadcaster.subscribe_to(subscribe).boxed())
                    .boxed())
            }
        }
    }
}

impl EventSubscriberConfig {
    pub fn to_subscriber(&self) -> Box<dyn sources::subscriber::Subscriber<Event>> {
        use EventSubscriberConfig::*;
        match self {
            Vs => unimplemented!(),
            HeadsOrTails => Box::new(crate::sources::subscriber::HeadsOrTailsEvents),
        }
    }
}

impl OutcomeSubscriberConfig {
    pub fn to_subscriber(
        &self,
        seed: &Seed,
    ) -> Box<dyn sources::subscriber::Subscriber<StampedOutcome>> {
        use OutcomeSubscriberConfig::*;
        match self {
            Vs => unimplemented!(),
            HeadsOrTails => Box::new(crate::sources::subscriber::HeadsOrTailsOutcomes {
                seed: seed.child(b"heads-or-tails-outcomes"),
            }),
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
