use super::*;
use crate::{core, curve::SchnorrImpl, db, sources};
use futures::StreamExt;
use std::{fs, sync::Arc};

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
    pub fn to_event_stream<C: core::Group>(
        &self,
        name: &str,
        logger: slog::Logger,
        db: Arc<dyn db::Db<C>>,
    ) -> anyhow::Result<sources::EventStream> {
        let name = name.to_owned();
        match self.clone() {
            EventSourceConfig::Redis(RedisConfig {
                connection_info,
                lists,
            }) => {
                info!(
                    logger,
                    "Connecting to redis://{}/{} to receive events for '{}'",
                    connection_info.addr, connection_info.db, name;
                );

                Ok(sources::redis::event_stream(
                    redis::Client::open(connection_info.clone())?,
                    lists,
                    logger.new(
                        o!("type" => "event_source", "name" => name, "source_type" => "redis"),
                    ),
                )?
                .boxed())
            }
            EventSourceConfig::TimeTicker {
                look_ahead,
                interval,
                initial_time,
            } => Ok(sources::time_ticker::time_events_stream(
                db.clone(),
                chrono::Duration::seconds(look_ahead as i64),
                chrono::Duration::seconds(interval as i64),
                initial_time.unwrap_or_else(|| {
                    use chrono::Timelike;
                    chrono::Utc::now()
                        .with_second(0)
                        .unwrap()
                        .with_nanosecond(0)
                        .unwrap()
                        .naive_utc()
                }),
                logger.new(
                    o!("type" => "event_source", "name" => name, "source_type" => "time_ticker"),
                ),
            )
            .boxed()),
            EventSourceConfig::ReEmitter { source, re_emitter } => {
                let stream = source.to_event_stream(&name, logger, db);
                let re_emitter = re_emitter.to_remitter();
                stream.map(|stream| re_emitter.re_emit_events(stream.boxed()).boxed())
            }
        }
    }
}

impl OutcomeSourceConfig {
    pub fn to_outcome_stream<C: core::Group>(
        &self,
        name: &str,
        seed: &Seed,
        logger: slog::Logger,
        db: Arc<dyn db::Db<C>>,
    ) -> anyhow::Result<sources::OutcomeStream> {
        use OutcomeSourceConfig::*;
        match self.clone() {
            Redis(RedisConfig {
                connection_info,
                lists,
            }) => {
                info!(
                    logger,
                    "Connecting to redis://{}/{} to receive outcomes for '{}'",
                    connection_info.addr, connection_info.db, name;
                );
                Ok(
                    sources::redis::event_stream(
                        redis::Client::open(connection_info.clone())?,
                        lists,
                        logger.new(o!("type" => "outcome_source", "name" => name.to_owned(), "source_type" => "redis"))
                    )?
                        .boxed()
                )
            }
            TimeTicker {} => {
                Ok(sources::time_ticker::time_outcomes_stream(
                    db.clone(),
                    logger.new(o!("type" => "outcome_source", "name" => name.to_owned(), "source_type" => "time_ticker"))
                ).boxed())
            }
            ReEmitter { source, re_emitter } => {
                let stream = source.to_outcome_stream(name, seed, logger, db);
                let re_emitter = re_emitter.to_remitter(name, seed);
                stream.map(|stream| re_emitter.re_emit_outcomes(stream.boxed()).boxed())
            }
        }
    }
}

impl EventReEmitterConfig {
    pub fn to_remitter(&self) -> Box<dyn sources::re_emitter::EventReEmitter> {
        use EventReEmitterConfig::*;
        match self {
            Vs => Box::new(crate::sources::re_emitter::Vs),
            HeadsOrTails => Box::new(crate::sources::re_emitter::HeadsOrTailsEvents),
        }
    }
}

impl OutcomeReEmitterConfig {
    pub fn to_remitter(
        &self,
        name: &str,
        seed: &Seed,
    ) -> Box<dyn sources::re_emitter::OutcomeReEmitter> {
        use OutcomeReEmitterConfig::*;
        match self {
            Vs => Box::new(crate::sources::re_emitter::Vs),
            HeadsOrTails => Box::new(crate::sources::re_emitter::HeadsOrTailsOutcomes {
                seed: seed
                    .child(b"heads-or-tails-outcomes")
                    .child(name.as_bytes()),
            }),
        }
    }
}

impl DbConfig {
    pub fn connect_database<C: core::Group>(
        &self,
    ) -> anyhow::Result<Arc<dyn db::Db<SchnorrImpl>>> {
        match self {
            DbConfig::InMemory => Ok(Arc::new(db::in_memory::InMemory::default())),
            DbConfig::Postgres { url } => {
                Ok(Arc::new(db::diesel::postgres::PgBackend::connect(&url)?))
            }
        }
    }
}
