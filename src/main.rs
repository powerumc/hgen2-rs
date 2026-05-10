mod config;
mod param;
mod runner;
mod transport;
mod vuser;

use crate::config::AppConfig;
use crate::runner::Runner;
use anyhow::Context;
use clap::{Args, Parser, Subcommand};
use log::{LevelFilter, info};
use simple_logger::SimpleLogger;
use std::fs;
use std::fs::File;
use std::path::PathBuf;

/// 기본 설정 yaml 문자열
const DEFAULT_CONFIG_YAML: &str = include_str!("../httpgen.config.yaml");

fn main() -> Result<(), anyhow::Error> {
    SimpleLogger::new()
        .with_level(LevelFilter::Info)
        .env()
        .without_timestamps()
        .init()?;

    let cli = Cli::parse();
    match cli.command {
        Some(Commands::Init) => return init(),
        Some(Commands::Run(opt)) => return run(opt),
        None => {}
    }

    Ok(())
}

#[derive(Parser)]
#[command(version, about, long_about = None, arg_required_else_help = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Init,
    Run(RunOpt),
}

#[derive(Args)]
struct RunOpt {
    #[arg(short = 'c', long)]
    config_file: Option<PathBuf>,

    #[arg(short = 'i', long, required = true)]
    interface: String,

    #[arg(long, default_value_t = 1)]
    eps: u64,

    #[arg(long)]
    vu: Option<usize>,
}

fn init() -> Result<(), anyhow::Error> {
    const FILENAME: &str = "httpgen.config.yaml";

    if fs::exists(FILENAME)? {
        return Err(anyhow::anyhow!("File already exists"));
    }

    fs::write(FILENAME, DEFAULT_CONFIG_YAML).context("failed to create config file")?;

    info!("Config file created at {}", FILENAME);

    Ok(())
}

fn run(opt: RunOpt) -> Result<(), anyhow::Error> {
    info!("Running on {}", opt.interface);

    let mut app_config: AppConfig = match opt.config_file {
        Some(config_file) => {
            info!("Loaded config file: {}", config_file.display());
            serde_yaml::from_reader(File::open(config_file)?)?
        }
        None => {
            info!("Loaded default config");
            serde_yaml::from_str(DEFAULT_CONFIG_YAML)?
        }
    };

    if let Some(vu) = opt.vu {
        anyhow::ensure!(vu > 0, "vu must be greater than 0");
        app_config.test.vu = vu;
    }

    let runner = Runner::new(app_config, opt.interface, opt.eps);
    runner.run()
}
