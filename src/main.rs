use alfred::updater_cli::{UpdateAction, run_default_update};
use alfred_eudic::{GITHUB_REPO, SearchArgs, WORKFLOW_ASSET_NAME, command::run_search};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "alfred-eudic")]
#[command(about = "Tool used to quickly search matched words by partial query")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Search {
        #[arg(long, env = "ALFRED_EUDIC_COMPLETION_FILE")]
        completion_file: Option<String>,
        #[arg(long, env = "ALFRED_EUDIC_DATABASE_FILE")]
        db_file: Option<String>,
        #[arg(default_value = "are")]
        spell: String,
    },
    Update {
        #[command(subcommand)]
        action: UpdateAction,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Search { completion_file, db_file, spell } => {
            run_search(SearchArgs { completion_file, db_file, spell }).await?
        }
        Commands::Update { action } => {
            run_default_update(GITHUB_REPO, WORKFLOW_ASSET_NAME, action).await?
        }
    }
    Ok(())
}
