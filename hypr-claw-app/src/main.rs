use clap::{Parser, Subcommand};
use anyhow::Result;

mod commands;
mod config;

#[derive(Parser)]
#[command(name = "hypr-claw")]
#[command(about = "Agent runtime with tool execution and sandboxing", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize Hypr-Claw configuration
    Init,
    /// Run an agent with a message
    Run {
        #[arg(long)]
        agent: String,
        #[arg(long)]
        user: String,
        #[arg(long)]
        message: String,
    },
    /// Check system health
    Health,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Init => commands::init::run().await,
        Commands::Run { agent, user, message } => {
            commands::run::run(&agent, &user, &message).await
        }
        Commands::Health => commands::health::run().await,
    }
}
