use clap::{AppSettings, IntoApp, Parser, Subcommand};
use clap_complete::{
    generate,
    shells::{Bash, Fish, Zsh},
};
use core::commands;
use std::path::PathBuf;
use utils::app_config::AppConfig;
use utils::error::Result;
use utils::types::LogLevel;

#[derive(Parser, Debug)]
#[clap(
    name = "gdp-router",
    author,
    about,
    long_about = "Rust GDP Router",
    version
)]
#[clap(setting = AppSettings::SubcommandRequired)]
#[clap(global_setting(AppSettings::DeriveDisplayOrder))]
pub struct Cli {
    /// Set a custom config file
    #[clap(short, long, parse(from_os_str), value_name = "FILE")]
    pub config: Option<PathBuf>,

    /// Set a custom config file
    #[clap(name = "debug", short, long = "debug", value_name = "DEBUG")]
    pub debug: Option<bool>,

    /// Set Log Level
    #[clap(
        name = "log_level",
        short,
        long = "log-level",
        value_name = "LOG_LEVEL"
    )]
    pub log_level: Option<LogLevel>,

    /// Set Net Interface
    #[clap(
        name = "net_interface",
        short,
        long = "net-interface",
        value_name = "eno1"
    )]
    pub net_interface: Option<String>,

    /// Set local router GDPName
    #[clap(name = "router_name", short='g', long = "router_name", value_name="ROUTER_NAME")]
    pub router_name: u32,

    /// Set remote RIB's Ipv4 Address to connect to
    #[clap(name = "target_rib", short, long = "target_rib", value_name="TARGET_RIB")]
    pub target_rib: Option<String>,

    /// Set remote RIB's GDPName
    #[clap(name = "rib_name", short, long = "rib_name", value_name="RIB_NAME")]
    pub rib_name: Option<u32>,

    /// Subcommands
    #[clap(subcommand)]
    command: Commands,

}

#[derive(Subcommand, Debug)]
enum Commands {
    #[clap(
        name = "router",
        about = "Run Router",
        long_about = None,
    )]
    Router,

    #[clap(
        name = "client",
        about = "Run test dtls client",
        long_about = None,
    )]
    Error,

    #[clap(
        name = "rib",
        about = "Run Remote RIB",
        long_about = None,
    )]
    Rib,

    #[clap(
        name = "completion",
        about = "Generate completion scripts",
        long_about = None,
        )]
    Completion {
        #[clap(subcommand)]
        subcommand: CompletionSubcommand,
    },

    #[clap(
        name = "config",
        about = "Show Configuration",
        long_about = None,
    )]
    Config,
}

#[derive(Subcommand, PartialEq, Debug)]
enum CompletionSubcommand {
    #[clap(about = "generate the autocompletion script for bash")]
    Bash,
    #[clap(about = "generate the autocompletion script for zsh")]
    Zsh,
    #[clap(about = "generate the autocompletion script for fish")]
    Fish,
}

pub fn cli_match() -> Result<()> {
    // Parse the command line arguments
    let cli = Cli::parse();

    // Merge clap config file if the value is set
    AppConfig::merge_config(cli.config.as_deref())?;

    let app = Cli::into_app();

    AppConfig::merge_args(app)?;

    // Execute the subcommand
    match &cli.command {
        Commands::Router => commands::router(cli.router_name, cli.target_rib, cli.rib_name)?,
        Commands::Error => commands::simulate_error()?,
        Commands::Rib => commands::rib( cli.target_rib)?,
        Commands::Completion { subcommand } => {
            let mut app = Cli::into_app();
            match subcommand {
                CompletionSubcommand::Bash => {
                    generate(Bash, &mut app, "gdp-router", &mut std::io::stdout());
                }
                CompletionSubcommand::Zsh => {
                    generate(Zsh, &mut app, "gdp-router", &mut std::io::stdout());
                }
                CompletionSubcommand::Fish => {
                    generate(Fish, &mut app, "gdp-router", &mut std::io::stdout());
                }
            }
        }
        Commands::Config => commands::config()?,
    }

    Ok(())
}
