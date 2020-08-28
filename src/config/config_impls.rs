use super::*;
use crate::{core, db, sources};
use futures::{Stream, StreamExt};
use std::{fs, sync::Arc};

impl LoggerConfig {
    pub fn to_slog_drain(&self) -> Result<RootDrain, Box<dyn std::error::Error>> {
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
    pub fn to_slog_drain(&self) -> Result<RootDrain, Box<dyn std::error::Error>> {
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
        name: &str,
        logger: slog::Logger,
        db: Arc<dyn db::Db>,
    ) -> Result<
        std::pin::Pin<Box<dyn Stream<Item = sources::Update<core::Event>> + Send>>,
        Box<dyn std::error::Error>,
    > {
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
                match re_emitter {
                    EventReEmitterConfig::Vs => {
                        let emitter = crate::sources::re_emitter::Vs;
                        stream.map(|stream| emitter.re_emit_events(stream).boxed())
                    }
                }
            }
        }
    }
}

impl OutcomeSourceConfig {
    pub fn to_outcome_stream(
        &self,
        name: &str,
        logger: slog::Logger,
        db: Arc<dyn db::Db>,
    ) -> Result<
        std::pin::Pin<Box<dyn Stream<Item = sources::Update<core::EventOutcome>> + Send>>,
        Box<dyn std::error::Error>,
    > {
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
                let stream = source.to_outcome_stream(&name, logger, db);
                match re_emitter {
                    OutcomeReEmitterConfig::Vs => {
                        let emitter = crate::sources::re_emitter::Vs;
                        stream.map(|stream| emitter.re_emit_outcomes(stream).boxed())
                    }
                }
            }
        }
    }
}
