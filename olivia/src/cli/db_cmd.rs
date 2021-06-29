use crate::{
    config::{Config, DbConfig},
    db::postgres::PgBackendWrite,
};
use anyhow::anyhow;

pub async fn init(config: Config) -> anyhow::Result<()> {
    match config.database {
        DbConfig::Postgres { url } => {
            let db = PgBackendWrite::connect(&url).await?;
            db.setup().await?;
        }
        _ => return Err(anyhow!("can only run init on a postgres database")),
    }
    Ok(())
}
