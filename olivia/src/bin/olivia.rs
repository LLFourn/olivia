use anyhow::Context;
use core::str::FromStr;
use olivia::{cli, config::Config};
use olivia_core::EventId;
use std::path::PathBuf;
use structopt::StructOpt;

extern crate tokio;

#[derive(Debug, StructOpt)]
#[structopt(name = "olivia")]
struct Opt {
    #[structopt(short, long, parse(from_os_str), name = "yaml config file")]
    config: PathBuf,
    #[structopt(subcommand)]
    cmd: Command,
}

#[derive(Debug, StructOpt)]
#[structopt(rename_all = "kebab-case")]
pub enum Command {
    Add(cli::add::Entity),
    Run,
    CheckConfig,
    Derive {
        event: String,
    },
    /// Database commands
    Db(Db),
}

#[derive(Debug, StructOpt)]
pub enum Db {
    Init,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = Opt::from_args();
    let config: Config = {
        use std::{fs::File, io::Read};
        let file_name = opt.config.to_str().unwrap_or("config file").to_owned();
        let mut file = File::open(opt.config)?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        serde_yaml::from_str(&content)
            .context(format!("{} is an invalid configuration file", file_name))?
    };

    match opt.cmd {
        Command::Add(entity) => cli::add::add(config, entity).await,
        Command::Run => cli::run::run(config).await,
        Command::Derive { event } => cli::derive::derive(config, EventId::from_str(&event)?),
        Command::Db(db) => match db {
            Db::Init => cli::db_cmd::init(config).await,
        },
        Command::CheckConfig => Ok(())
    }
}
