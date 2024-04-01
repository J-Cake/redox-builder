use std::env;
use std::path::PathBuf;
use std::sync::OnceLock;
use clap::{Parser, Subcommand};
use build::build;
use checkout::checkout;

use hub::error::*;
use hub::reporter::*;

#[derive(Debug, Parser)]
#[clap(version, about)]
pub struct Args {
    #[arg(long, default_value_t = ReportMode::Auto, value_enum)]
    pub report_mode: ReportMode,

    #[command(subcommand)]
    pub action: BuildActions,
}

#[derive(Debug, Subcommand)]
pub enum BuildActions {
    /// Builds the provided configuration
    Build {
        #[arg(index = 1)]
        config: PathBuf,

        #[arg(long, short, action, default_value_t = false)]
        clean: bool,

        #[arg(long = "build-in", required = false)]
        build_dir: Option<PathBuf>,
    },

    /// Extracts a particular recipe's source to a defined destination
    Checkout {
        #[arg(index = 1)]
        recipe: String,

        #[arg(long, short)]
        destination: Option<PathBuf>,
    },
}

pub static REPORTER: OnceLock<Reporter> = OnceLock::new();

#[tokio::main]
pub async fn main() -> Result<()> {
    env_logger::init();

    let args = Args::parse();
    REPORTER
        .set(Reporter::new(args.report_mode))
        .expect("Unable to set reporter");

    match args.action {
        BuildActions::Build {
            clean,
            config,
            build_dir,
        } => {
            build(
                match config.is_absolute() {
                    true => config,
                    false => env::current_dir()?.join(config),
                },
                clean,
                build_dir,
            )
                .await?
        }
        BuildActions::Checkout {
            destination,
            recipe,
        } => checkout(recipe, destination).await?,
    }

    Ok(())
}
