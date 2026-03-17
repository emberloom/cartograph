use cartograph::{historian, parser, policy, query, server, store};
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
    CoChanges { entity: String },
    /// Show who owns an entity (git blame)
    WhoOwns { entity: String },
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
    /// Policy as code: check architectural rules against the graph
    Policy {
        #[command(subcommand)]
        command: PolicyCommands,
    },
}

#[derive(Subcommand)]
enum PolicyCommands {
    /// Check policies against the indexed graph
    Check {
        /// Path to the policy YAML config file
        #[arg(short, long, default_value = "policies.yaml")]
        config: String,
        /// Output format: text, json, sarif
        #[arg(short, long, default_value = "text")]
        format: String,
    },
    /// Generate a starter policy configuration file
    Init {
        /// Output path for the generated config
        #[arg(short, long, default_value = "policies.yaml")]
        output: String,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    // Ensure DB directory exists
    let db_path = std::path::Path::new(&cli.db);
    if let Some(parent) = db_path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }

    let conn = rusqlite::Connection::open(&cli.db)?;
    store::schema::init_db(&conn)?;

    match cli.command {
        Commands::Index => {
            let repo_path = std::path::Path::new(&cli.repo).canonicalize()?;
            let mut store = store::graph::GraphStore::new(conn)?;

            println!("Indexing {}...", repo_path.display());

            // Layer 1: Structure
            let (rs_count, ts_count) = parser::index_repo(&repo_path, &mut store)?;
            if ts_count > 0 {
                println!(
                    "  Structure: {} Rust files, {} TypeScript files",
                    rs_count, ts_count
                );
            } else {
                println!("  Structure: {} Rust files", rs_count);
            }

            // Layer 2: Dynamics
            match historian::mine_commits(&repo_path, None) {
                Ok(commits) => {
                    println!("  Git history: {} commits", commits.len());

                    let cochanges = historian::analyze_cochanges(&commits);
                    historian::write_cochange_edges(&mut store, &cochanges)?;
                    println!("  Co-changes: {} pairs", cochanges.len());

                    match historian::write_ownership_edges(&mut store, &repo_path) {
                        Ok(()) => println!("  Ownership: done"),
                        Err(e) => println!("  Ownership: skipped ({})", e),
                    }
                }
                Err(e) => {
                    println!("  Git history: skipped ({})", e);
                    println!("  Co-changes: skipped (no git history)");
                    println!("  Ownership: skipped (no git history)");
                }
            }

            println!("Index complete.");
        }
        Commands::BlastRadius { entity, depth } => {
            let store = store::graph::GraphStore::new(conn)?;
            let results = query::blast_radius::query(&store, &entity, depth);

            if results.is_empty() {
                println!("No results for '{entity}'");
            } else {
                println!("{:<40} {:<10} EDGE", "ENTITY", "DEPTH");
                println!("{}", "-".repeat(60));
                for r in &results {
                    let path = r.entity_path.as_deref().unwrap_or(&r.entity_name);
                    println!("{:<40} {:<10} {}", path, r.depth, r.edge_kind);
                }
            }
        }
        Commands::Deps { entity, direction } => {
            let store = store::graph::GraphStore::new(conn)?;
            let dir = match direction.as_str() {
                "upstream" => petgraph::Direction::Incoming,
                _ => petgraph::Direction::Outgoing,
            };

            if let Some(e) = store.find_entity_by_path(&entity) {
                let deps = store.dependencies(&e.id, dir);
                println!("{:<40} KIND", "ENTITY");
                println!("{}", "-".repeat(50));
                for d in &deps {
                    let path = d.path.as_deref().unwrap_or(&d.name);
                    println!("{:<40} {:?}", path, d.kind);
                }
            } else {
                println!("Entity not found: {entity}");
            }
        }
        Commands::CoChanges { entity } => {
            let store = store::graph::GraphStore::new(conn)?;
            let results = query::co_changes(&store, &entity);

            if results.is_empty() {
                println!("No co-change data for '{entity}'");
            } else {
                println!("{:<40} CONFIDENCE", "ENTITY");
                println!("{}", "-".repeat(55));
                for r in &results {
                    let path = r.entity_path.as_deref().unwrap_or(&r.entity_name);
                    println!("{:<40} {:.2}", path, r.confidence);
                }
            }
        }
        Commands::WhoOwns { entity } => {
            let store = store::graph::GraphStore::new(conn)?;
            let results = query::ownership::query(&store, &entity);

            if results.is_empty() {
                println!("No ownership data for '{entity}'");
            } else {
                println!("{:<30} CONFIDENCE", "OWNER");
                println!("{}", "-".repeat(45));
                for r in &results {
                    println!("{:<30} {:.2}", r.entity_name, r.confidence);
                }
            }
        }
        Commands::Hotspots { limit } => {
            let store = store::graph::GraphStore::new(conn)?;
            let results = query::hotspots::query(&store, limit);

            if results.is_empty() {
                println!("No hotspot data found. Run 'index' first.");
            } else {
                println!("{:<40} CONNECTIONS", "ENTITY");
                println!("{}", "-".repeat(55));
                for r in &results {
                    let path = r.entity_path.as_deref().unwrap_or(&r.entity_name);
                    println!("{:<40} {}", path, r.edge_count);
                }
            }
        }
        Commands::Serve { .. } => {
            let store = store::graph::GraphStore::new(conn)?;
            server::run_mcp_server(store)?;
        }
        Commands::Policy { command } => match command {
            PolicyCommands::Check { config, format } => {
                let store = store::graph::GraphStore::new(conn)?;
                let yaml = std::fs::read_to_string(&config)
                    .map_err(|e| anyhow::anyhow!("Failed to read config '{}': {}", config, e))?;
                let policy_config = policy::rules::parse_policy_config(&yaml)?;
                let validation_errors = policy_config.validate();
                if !validation_errors.is_empty() {
                    eprintln!("Policy config validation errors:");
                    for err in &validation_errors {
                        eprintln!("  - {}", err);
                    }
                    anyhow::bail!("Invalid policy configuration");
                }
                let result = policy::engine::evaluate(&store, &policy_config);
                let output = match format.as_str() {
                    "json" => policy::report::format_json(&result),
                    "sarif" => policy::report::format_sarif(&result),
                    _ => policy::report::format_report(&result),
                };
                print!("{}", output);
                if result.has_errors {
                    std::process::exit(1);
                }
            }
            PolicyCommands::Init { output } => {
                let config = policy::rules::generate_starter_config();
                std::fs::write(&output, &config)?;
                println!("Generated starter policy config: {}", output);
            }
        },
    }

    Ok(())
}
