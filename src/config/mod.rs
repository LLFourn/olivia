use crate::seed::Seed;
use chrono::NaiveDateTime;
use std::collections::HashMap;

mod config_impls;

pub type RootDrain = Box<
    dyn slog::SendSyncRefUnwindSafeDrain<Err = slog::Never, Ok = ()>
        + 'static
        + std::panic::UnwindSafe,
>;

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub events: HashMap<String, EventSourceConfig>,
    #[serde(default)]
    pub outcomes: HashMap<String, OutcomeSourceConfig>,
    #[serde(default)]
    pub database: DbConfig,
    #[serde(default)]
    pub loggers: LoggersConfig,
    pub secret_seed: Option<Seed>,
    pub rest_api: Option<RestConfig>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RestConfig {
    pub listen: std::net::SocketAddr,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct RedisConfig {
    #[serde(deserialize_with = "deser_redis_connection_info", rename = "url")]
    pub connection_info: redis::ConnectionInfo,
    pub lists: Vec<String>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case", tag = "backend")]
#[serde(deny_unknown_fields)]
pub enum DbConfig {
    Postgres { url: String },
    InMemory,
}

impl Default for DbConfig {
    fn default() -> Self {
        DbConfig::InMemory
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case", tag = "type")]
#[serde(deny_unknown_fields)]
pub enum EventSourceConfig {
    TimeTicker {
        interval: u32,
        look_ahead: u32,
        initial_time: Option<NaiveDateTime>,
    },
    Redis(RedisConfig),
    ReEmitter {
        source: Box<EventSourceConfig>,
        re_emitter: ReEmitterConfig,
    },
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case", tag = "type")]
#[serde(deny_unknown_fields)]
pub enum ReEmitterConfig {
    VsReEmitter,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case", tag = "type")]
#[serde(deny_unknown_fields)]
pub enum OutcomeSourceConfig {
    TimeTicker {},
    Redis(RedisConfig),
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case", tag = "type")]
#[serde(deny_unknown_fields)]
pub enum LoggerConfig {
    Stdout {
        #[serde(default)]
        color: Option<bool>,
    },
    Stderr {
        #[serde(default)]
        color: Option<bool>,
    },
    File {
        path: String,
    },
}

#[derive(Debug, Deserialize, Clone)]
pub struct LoggersConfig(Vec<LoggerConfig>);

impl Default for LoggersConfig {
    fn default() -> Self {
        LoggersConfig(vec![LoggerConfig::Stdout { color: None }])
    }
}

pub fn deser_redis_connection_info<'a, D: serde::Deserializer<'a>>(
    d: D,
) -> Result<redis::ConnectionInfo, D::Error> {
    struct MyVisitor;

    impl<'a> serde::de::Visitor<'a> for MyVisitor {
        type Value = redis::ConnectionInfo;

        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(
                f,
                "a redis connection uri like redis://p%40ssw0rd@myredis.com:16379/0"
            )
        }

        fn visit_str<E: serde::de::Error>(self, data: &str) -> Result<Self::Value, E> {
            use redis::IntoConnectionInfo;
            data.into_connection_info()
                .map_err(|e| serde::de::Error::custom(e))
        }
    }

    d.deserialize_str(MyVisitor)
}
