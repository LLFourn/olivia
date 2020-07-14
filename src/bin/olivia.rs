use core::str::FromStr;
use olivia::{cli, config::Config, core::EventId};
use std::path::PathBuf;
use structopt::StructOpt;

extern crate tokio;

#[derive(Debug, StructOpt)]
#[structopt(name = "basic")]
struct Opt {
    #[structopt(short, long, parse(from_os_str), name = "toml config file")]
    config: PathBuf,
    #[structopt(subcommand)]
    cmd: Command,
}

#[derive(Debug, StructOpt)]
pub enum Command {
    Run,
    Derive { event: String },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opt = Opt::from_args();
    let config: Config = {
        use std::{fs::File, io::Read};
        let mut file = File::open(opt.config)?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        serde_yaml::from_str(&content)?
    };

    match opt.cmd {
        Command::Run => cli::run::run(config),
        Command::Derive { event } => cli::derive::derive(config, EventId::from_str(&event)?),
    }
}
