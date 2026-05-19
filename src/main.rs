use alfred::updater_cli::{UpdateAction, run_default_update};
use alfred_eudic::{GITHUB_REPO, SearchArgs, WORKFLOW_ASSET_NAME, command::{run_search, run_card_update}};
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
    /// Perform a search query
    Search {
        /// File used for completion items
        #[arg(long, env = "ALFRED_EUDIC_COMPLETION_FILE")]
        completion_file: Option<String>,

        /// Database file used for explanation (ECDICT stardict)
        #[arg(long, env = "ALFRED_EUDIC_DATABASE_FILE")]
        db_file: Option<String>,

        /// Spell of the word you want to query
        #[arg(default_value = "are")]
        spell: String,
    },
    /// Update workflow
    Update {
        #[command(subcommand)]
        action: UpdateAction,
    },
    /// Internal: rebuild the Quick Look card for a word after a slow LLM
    /// finishes. Intended to be spawned as a background subprocess from
    /// the main `Search` flow; never invoked directly by Alfred.
    CardUpdate {
        #[arg(long, env = "ALFRED_EUDIC_COMPLETION_FILE")]
        completion_file: Option<String>,
        #[arg(long, env = "ALFRED_EUDIC_DATABASE_FILE")]
        db_file: Option<String>,
        spell: String,
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
        Commands::CardUpdate { completion_file, db_file, spell } => {
            run_card_update(SearchArgs { completion_file, db_file, spell }).await?
        }
    }
    Ok(())
}
