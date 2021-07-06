use crate::{config::Config, log::OracleLog, oracle::Oracle, sources::Update};
use futures::{future::FutureExt, stream, stream::StreamExt};
use olivia_core::{Event, StampedOutcome, Node};

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
            let read_conn = config.database.connect_database_read().await?;
            let event_streams = config.build_event_streams(read_conn.clone(), logger.clone())?;
            let outcome_streams =
                config.build_outcome_streams(read_conn, &secret_seed, logger.clone())?;
            let node_streams = config.build_node_streams(logger.clone())?;

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
                        dbg!(&event_id);
                        oracle
                            .add_event(event)
                            .map(move |res| {
                                if let Some(processed_notifier) = processed_notifier {
                                    let _ = processed_notifier.send(res.is_err());
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
                            o!("type" => "new_outcome", "event_id" => stamped.outcome.id.to_string(), "value" => stamped.outcome.value.to_string()),
                        );
                        oracle.complete_event(stamped).map(move |res| {
                            if let Some(processed_notifier) = processed_notifier {
                                let _ = processed_notifier.send(res.is_err());
                            }
                            logger.log_outcome_result(res);
                        })
                    },
                )
                .inspect(|_| warn!(logger, "Outcome processing has stopped"))
                .boxed();

            // Processing namespace updates
            let node_loop = stream::select_all(node_streams)
                .for_each(| Update { update: node, processed_notifier}: Update<Node> | {
                    let logger = logger.new(
                        o!("type" => "new_node", "path" => node.path.to_string()),
                    );

                    db.insert_node(node).map(move |res| {
                       if let Some(processed_notifier) = processed_notifier {
                           let _ = processed_notifier.send(res.is_err());
                       }
                       match res {
                           Ok(()) => {
                               info!(logger, "added");
                           },
                           Err(e) => error!(logger, "{}", e),
                       }
                    })
                })
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
