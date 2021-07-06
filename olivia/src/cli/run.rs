use crate::{
    broadcaster::Broadcaster, config::Config, log::OracleLog, oracle::Oracle, sources::Update,
};
use futures::{future::FutureExt, stream, stream::StreamExt};
use olivia_core::{Event, Node, StampedOutcome};
use std::sync::Arc;

pub async fn run(config: Config) -> anyhow::Result<()> {
    let logger = slog::Logger::root(config.loggers.to_slog_drain()?, o!());
    let db = config.database.connect_database().await?;

    let mut services = vec![];

    if let Some(rest_config) = &config.rest_api {
        info!(logger, "starting http server on {}", rest_config.listen);
        let rest_api_server = warp::serve(crate::rest_api::routes(
            config.database.connect_database_read_group().await?,
            logger.new(o!("type" => "http")),
        ))
        .run(rest_config.listen)
        .inspect(|_| info!(logger, "HTTP API server has shut down"))
        .boxed();

        services.push(rest_api_server);
    }

    // If we have the secret seed then we are running an attesting oracle
    match &config.secret_seed {
        Some(secret_seed) => {
            let mut event_broadcaster = Broadcaster::default();
            let mut outcome_broadcaster = Broadcaster::default();

            let read_conn = config.database.connect_database_read().await?;
            let event_streams = config.build_event_streams(
                read_conn.clone(),
                logger.clone(),
                &mut event_broadcaster,
            )?;
            let outcome_streams = config.build_outcome_streams(
                read_conn,
                &secret_seed.child(b"outcome-seed"),
                logger.clone(),
                &mut outcome_broadcaster,
            )?;
            let node_streams = config.build_node_streams(logger.clone())?;

            let event_broadcaster = Arc::new(event_broadcaster);
            let outcome_broadcaster = Arc::new(outcome_broadcaster);

            let oracle = Oracle::new(secret_seed.clone(), db.clone()).await?;

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
                            .new(o!("type" => "new_event", "event_id" => event_id.to_string()));
                        let event_broadcaster = event_broadcaster.clone();

                        oracle
                            .add_event(event.clone())
                            .map(move |res| {
                                if let Some(processed_notifier) = processed_notifier {
                                    let _ = processed_notifier.send(res.is_err());
                                }
                                if res.is_ok() {
                                    event_broadcaster.process(event.id.clone().path(), event);
                                }
                                logger.log_event_result(res);
                            })
                            .boxed()
                    },
                )
                .inspect(|_| warn!(logger, "Event processing has stopped"))
                .boxed();

            // Processing outcomes
            let outcome_loop = stream::select_all(outcome_streams)
                .for_each(
                    |Update {
                         update: stamped,
                         processed_notifier,
                     }: Update<StampedOutcome>| {
                        let logger = logger.new(
                            o!("type" => "new_outcome", "event_id" => stamped.outcome.id.to_string(), "value" => stamped.outcome.outcome_string()),
                        );
                        let outcome_broadcaster = outcome_broadcaster.clone();
                        oracle.complete_event(stamped.clone()).map(move |res| {
                            if let Some(processed_notifier) = processed_notifier {
                                let _ = processed_notifier.send(res.is_err());
                            }
                            if res.is_ok() {
                                outcome_broadcaster
                                    .process(stamped.outcome.id.clone().path(), stamped);
                            }
                            logger.log_outcome_result(res);
                        })
                    },
                )
                .inspect(|_| warn!(logger, "Outcome processing has stopped"))
                .boxed();

            // Processing namespace updates
            let node_loop = stream::select_all(node_streams)
                .for_each(
                    |Update {
                         update: node,
                         processed_notifier,
                     }: Update<Node>| {
                        let logger =
                            logger.new(o!("type" => "new_node", "path" => node.path.to_string()));

                        db.insert_node(node).map(move |res| {
                            if let Some(processed_notifier) = processed_notifier {
                                let _ = processed_notifier.send(res.is_err());
                            }
                            match res {
                                Ok(()) => {
                                    info!(logger, "added");
                                }
                                Err(e) => error!(logger, "{}", e),
                            }
                        })
                    },
                )
                .inspect(|_| warn!(logger, "Node processing has stopped"))
                .boxed();

            // This solves a lifetime issue
            let mut services = services;

            services.push(event_loop);
            services.push(outcome_loop);
            services.push(node_loop);

            futures::future::join_all(services).await;
        }
        None => {
            // otherwise we are just running an api server
            futures::future::join_all(services).await;
        }
    }

    Ok(())
}
