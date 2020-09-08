use crate::{
    config::Config,
    core::{Event, EventOutcome},
    db::Db,
    log::OracleLog,
    oracle::Oracle,
    sources::Update,
    curve::CurveImpl
};
use futures::{future::FutureExt, stream, stream::StreamExt};
use std::sync::Arc;

pub fn run(config: Config) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut rt = tokio::runtime::Runtime::new()?;

    let logger = slog::Logger::root(config.loggers.to_slog_drain()?, o!());
    let db: Arc<dyn Db<CurveImpl>> = config.database.connect_database::<CurveImpl>()?;

    let event_streams: Vec<_> = config
        .events
        .iter()
        .map(|(name, source)| source.to_event_stream(name, logger.clone(), db.clone()))
        .collect::<Result<Vec<_>, _>>()?;

    let outcome_streams = config
        .outcomes
        .iter()
        .map(|(name, source)| source.to_outcome_stream(name, logger.clone(), db.clone()))
        .collect::<Result<Vec<_>, _>>()?;

    let mut services = vec![];

    if let Some(rest_config) = config.rest_api {
        let rest_api_server = warp::serve(crate::rest_api::routes(db.clone()))
            .run(rest_config.listen)
            .boxed();

        services.push(rest_api_server);
    }

    // If we have the secret seed then we are running an attesting oracle
    if let Some(secret_seed) = config.secret_seed {
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
                     update: outcome,
                     processed_notifier,
                 }: Update<EventOutcome>| {
                    let logger = logger.new(
                        o!("type" => "new_outcome", "event_id" => format!("{}", outcome.event_id)),
                    );
                    oracle.complete_event(outcome).map(move |res| {
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
    } else {
        // otherwise we are just running an api server
        rt.block_on(futures::future::join_all(services));
    }

    Ok(())
}
