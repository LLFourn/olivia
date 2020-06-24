use crate::sources::Update;
use futures::{channel::mpsc, stream};
use redis::RedisResult;
use serde::de::DeserializeOwned;
use serde_json;
use std::{
    collections::{hash_map::RandomState, HashSet},
    iter::FromIterator,
    thread,
};

pub fn event_stream<StrList: IntoIterator<Item = String>, I: DeserializeOwned + Send + 'static>(
    client: redis::Client,
    lists: StrList,
    logger: slog::Logger,
) -> Result<impl stream::Stream<Item = Update<I>>, redis::RedisError> {
    let (sender, receiver) = mpsc::unbounded();
    let mut blpop = redis::cmd("BLPOP");
    let mut conn = client.get_connection()?;

    let lists: HashSet<String, RandomState> = HashSet::from_iter(lists);

    for channel in lists {
        blpop.arg(channel);
    }

    // set no timeout
    blpop.arg(0);

    thread::spawn(move || loop {
        let result: RedisResult<(String, String)> = blpop.query(&mut conn);
        match result {
            Ok((list_name, json)) => match serde_json::from_str::<I>(&json) {
                Ok(item) => {
                    if sender
                        .unbounded_send(Update {
                            update: item,
                            processed_notifier: None,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
                Err(e) => {
                    crit!(
                        logger,
                        "Failed to deserialize event";
                        "list_name" => list_name,
                        "error" => format!("{}",e),
                        "json" => format!("{}",json)
                    );
                }
            },
            Err(e) => {
                match e.kind() {
                    redis::ErrorKind::TypeError => {
                        crit!(logger, "Event was an invalid String"; "error" => format!("{}", e));
                    }
                    _ => {
                        crit!(logger, "Unable to handle Error from Redis. Reconnecting."; "error" => format!("{}", e));
                        // TODO: Maybe just do a trivial cmd to see if it works
                        loop {
                            match client.get_connection() {
                                Ok(new_conn) => {
                                    conn = new_conn;
                                    info!(logger, "Attempting to reconnect to Redis");
                                    break;
                                }
                                Err(e) => {
                                    error!(logger,"Failed to re-connect to Redis. Trying again in 5 seconds"; "error" => format!("{}", e));
                                    thread::sleep(std::time::Duration::from_millis(5_000));
                                }
                            }
                        }
                    }
                }
            }
        }

        info!(
            logger,
            "Redis loop has shut down because channel has been dropped"
        );
    });

    Ok(receiver)
}
