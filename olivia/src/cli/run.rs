use crate::{config::Config, oracle::Oracle, oracle_loop::OracleLoop};
use core::{
    future::{self, Future},
    pin::Pin,
};

pub async fn run(config: Config) -> anyhow::Result<()> {
    let logger = slog::Logger::root(config.loggers.to_slog_drain()?, o!());
    let db = config.database.connect_database().await?;

    let rest_server: Pin<Box<dyn Future<Output = _>>> = match &config.rest_api {
        Some(rest_config) => {
            let logger = logger.new(o!("type" => "http"));
            info!(logger, "starting http server on {}", rest_config.listen);
            let rest_api_server = warp::serve(crate::rest_api::routes(
                config.database.connect_database_read_group().await?,
                logger.clone(),
            ))
            .run(rest_config.listen);

            Box::pin(tokio::spawn(async move {
                rest_api_server.await;
                info!(logger, "http API server has shut down");
            }))
        }
        None => Box::pin(future::ready(Ok(()))),
    };

    let oracle_loop: Pin<Box<dyn Future<Output = _>>> = match &config.secret_seed {
        Some(secret_seed) => {
            let read_conn = config.database.connect_database_read().await?;
            let events = config.build_event_streams(read_conn.clone(), logger.clone())?;
            let outcomes = config.build_outcome_streams(
                read_conn,
                &secret_seed.child(b"outcome-seed"),
                logger.clone(),
            )?;

            let nodes = config.build_node_streams(logger.clone())?;

            let oracle = Oracle::new(secret_seed.clone(), db.clone()).await?;

            Box::pin(tokio::spawn(
                OracleLoop {
                    events,
                    outcomes,
                    nodes,
                    oracle,
                    db,
                    logger: logger.clone(),
                }
                .start(),
            ))
        }
        None => Box::pin(future::ready(Ok(()))),
    };

    let _ = tokio::join!(rest_server, oracle_loop);
    info!(logger, "olivia stopping");
    Ok(())
}
