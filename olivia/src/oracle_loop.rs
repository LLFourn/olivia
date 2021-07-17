use crate::{
    db::Db,
    log::OracleLog,
    sources::{self, Update},
    Oracle,
};
use olivia_core::{Event, Group, Node, Path, PrefixPath, StampedOutcome};
use std::sync::Arc;
use tokio_stream::{StreamExt, StreamMap};

pub struct OracleLoop<G: Group> {
    pub events: StreamMap<Path, sources::Stream<Event>>,
    pub outcomes: StreamMap<Path, sources::Stream<StampedOutcome>>,
    pub nodes: StreamMap<Path, sources::Stream<Node>>,
    pub oracle: Oracle<G>,
    pub db: Arc<dyn Db<G>>,
    pub logger: slog::Logger,
}

impl<G: Group> OracleLoop<G> {
    pub async fn start(self) {
        let OracleLoop {
            mut events,
            mut outcomes,
            mut nodes,
            oracle,
            db,
            logger,
        } = self;
        loop {
            tokio::select! {
                Some((parent, Update { update: event, processed_notifier })) = events.next() => {
                    let event = event.prefix_path(parent.as_path_ref());
                    let logger = logger
                        .new(o!("type" => "new_event", "event_id" => event.id.to_string()));
                    let res = oracle.add_event(event).await;
                    if let Some(processed_notifier) = processed_notifier {
                        let _ = processed_notifier.send(res.is_err());
                    }
                    logger.log_event_result(res)
                },
                Some((parent, Update { update: stamped, processed_notifier })) = outcomes.next() => {
                    let stamped = stamped.prefix_path(parent.as_path_ref());
                    let logger = logger.new(
                            o!("type" => "new_outcome", "event_id" => stamped.outcome.id.to_string(), "value" => stamped.outcome.outcome_string()),
                        );
                    let res = oracle.complete_event(stamped.clone()).await;
                    if let Some(processed_notifier) = processed_notifier {
                        let _ = processed_notifier.send(res.is_err());
                    }
                    logger.log_outcome_result(res)
                },
                Some((parent, Update { update: node, processed_notifier })) = nodes.next() => {
                    let node = node.prefix_path(parent.as_path_ref());
                    let logger =
                        logger.new(o!("type" => "new_node", "path" => node.path.to_string()));
                    let res = db.set_node(node.clone()).await;
                    if let Some(processed_notifier) = processed_notifier {
                        let _ = processed_notifier.send(res.is_err());
                    }

                    match res {
                        Ok(()) => info!(logger, "added"),
                        Err(e) => error!(logger, "failed to add"; "error" => e.to_string()),
                    }
                },
                else =>  {
                    info!(logger, "stopping oracle loop");
                    break;
                }
            }
        }
    }
}
