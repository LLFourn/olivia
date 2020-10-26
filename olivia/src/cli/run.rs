use crate::{
    config::Config,
    core::{Event, StampedOutcome},
    curve::SchnorrImpl,
    db::Db,
    log::OracleLog,
    oracle::Oracle,
    sources::Update,
};
use futures::{future::FutureExt, stream, stream::StreamExt};
use std::sync::Arc;

pub fn run(config: Config) -> anyhow::Result<()> {
    let mut rt = tokio::runtime::Runtime::new()?;

    let logger = slog::Logger::root(config.loggers.to_slog_drain()?, o!());
    let db: Arc<dyn Db<SchnorrImpl>> = config.database.connect_database::<SchnorrImpl>()?;

    let mut services = vec![];

    if let Some(rest_config) = config.rest_api {
        info!(logger, "starting http server on {}", rest_config.listen);
        let rest_api_server = warp::serve(crate::rest_api::routes(
            db.clone(),
            logger.new(o!("type" => "http")),
        ))
        .run(rest_config.listen)
        .boxed();

        services.push(rest_api_server);
    }

    // If we have the secret seed then we are running an attesting oracle
    match config.secret_seed {
        Some(secret_seed) => {
            let event_streams: Vec<_> = config
                .events
                .iter()
                .map(|(name, source)| source.to_event_stream(name, logger.clone(), db.clone()))
                .collect::<Result<Vec<_>, _>>()?;

            let outcome_streams = config
                .outcomes
                .iter()
                .map(|(name, source)| {
                    source.to_outcome_stream(name, &secret_seed, logger.clone(), db.clone())
                })
                .collect::<Result<Vec<_>, _>>()?;

            let oracle = rt.block_on(Oracle::new(secret_seed, db.clone()))?;

            // Processing new events
            let event_loop = stream::select_all(event_streams)
                .for_each(
                    // FIXME: make this an async function
                    |Update {
                         update: event,
                         processed_notifier,
                     }: Update<Event>| {
                        let event_id = event.id.clone();
                        let logger = logger
                            .new(o!("type" => "new_event", "event_id" => format!("{}", &event_id)));

                        oracle
                            .add_event(event)
                            .map(move |res| {
                                logger.log_event_result(res);
                                if let Some(processed_notifier) = processed_notifier {
                                    let _ = processed_notifier.send(());
                                }
                            })
                            .boxed()
                    },
                )
                .boxed();

            // Processing outcomes
            let outcome_loop = stream::select_all(outcome_streams)
                .for_each(
                    |Update {
                         update: stamped,
                         processed_notifier,
                     }: Update<StampedOutcome>| {
                        let logger = logger.new(
                            o!("type" => "new_outcome", "event_id" => stamped.outcome.id.to_string(), "value" => stamped.outcome.value.to_string()),
                        );
                        oracle.complete_event(stamped).map(move |res| {
                            logger.log_outcome_result(res);
                            if let Some(processed_notifier) = processed_notifier {
                                let _ = processed_notifier.send(());
                            }
                        })
                    },
                )
                .boxed();

            // This solves a lifetime issue
            let mut services = services;

            services.push(event_loop);
            services.push(outcome_loop);

            rt.block_on(futures::future::join_all(services));
        }
        None => {
            // otherwise we are just running an api server
            rt.block_on(futures::future::join_all(services));
        }
    }

    Ok(())
}
