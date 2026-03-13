use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "cartograph")]
#[command(about = "Emberloom Cartograph — codebase world model")]
struct Cli {
    /// Path to the repository to analyze
    #[arg(short, long, default_value = ".")]
    repo: String,

    /// Path to the Cartograph database
    #[arg(short, long, default_value = ".cartograph/db.sqlite")]
    db: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Index a repository (parse structure + mine git history)
    Index,
    /// Query the dependency graph
    Deps {
        /// Entity to query (file path or module name)
        entity: String,
        /// Direction: upstream or downstream
        #[arg(short, long, default_value = "downstream")]
        direction: String,
    },
    /// Show blast radius for an entity
    BlastRadius {
        entity: String,
        /// Maximum traversal depth
        #[arg(short, long, default_value = "3")]
        depth: usize,
    },
    /// Show files that co-change with an entity
    CoChanges {
        entity: String,
    },
    /// Show who owns an entity (git blame)
    WhoOwns {
        entity: String,
    },
    /// Show change hotspots
    Hotspots {
        /// Number of results
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },
    /// Start MCP server (stdio transport for agent consumption)
    Serve {
        /// Use stdio transport (default, for Claude Code / Codex)
        #[arg(long, default_value = "true")]
        stdio: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    println!("Cartograph v{}", env!("CARGO_PKG_VERSION"));
}
