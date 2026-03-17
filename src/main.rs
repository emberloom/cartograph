use cartograph::{
    coverage, historian, integrations, parser, policy, prediction, query, server, store,
};
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

    // ── New commands ────────────────────────────────────────────────────
    /// Predict regression risk from a set of changed files
    Predict {
        /// Comma-separated list of changed file paths
        #[arg(long)]
        changed: String,
        /// Max results to show
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },

    /// Analyze a PR (list of changed files) and produce a report
    PrAnalysis {
        /// Comma-separated list of changed file paths
        #[arg(long)]
        changed: String,
    },

    /// Generate a CI report for changed files
    CiReport {
        /// Comma-separated list of changed file paths
        #[arg(long)]
        changed: String,
        /// Output format: sarif, github-actions, json
        #[arg(short, long, default_value = "json")]
        format: String,
        /// Fail threshold: none, low, medium, high, critical
        #[arg(long, default_value = "none")]
        fail_on: String,
    },

    /// Import and query test coverage data
    Coverage {
        #[command(subcommand)]
        subcmd: CoverageCommands,
    },

    /// Check architectural policies against the graph
    Policy {
        #[command(subcommand)]
        subcmd: PolicyCommands,
    },
}

#[derive(Subcommand)]
enum CoverageCommands {
    /// Import coverage data from a file
    Import {
        /// Path to the coverage file
        #[arg(long)]
        file: String,
        /// Format: lcov, json (auto-detected if not specified)
        #[arg(long)]
        format: Option<String>,
    },
    /// Show coverage report
    Report,
    /// Show coverage gaps (hotspots with low coverage)
    Gaps {
        /// Minimum connections for a file to be considered
        #[arg(long, default_value = "3")]
        min_connections: usize,
        /// Max results
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },
}

#[derive(Subcommand)]
enum PolicyCommands {
    /// Check policies from a YAML config file
    Check {
        /// Path to the policy YAML file
        #[arg(long)]
        config: String,
    },
    /// Generate a starter policy config
    Init,
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

        // ── New commands ────────────────────────────────────────────────
        Commands::Predict { changed, limit } => {
            let store = store::graph::GraphStore::new(conn)?;
            let changed_files: Vec<String> =
                changed.split(',').map(|s| s.trim().to_string()).collect();
            let config = prediction::PredictionConfig {
                max_results: limit,
                ..prediction::PredictionConfig::default()
            };
            let predictions =
                prediction::scoring::predict_regressions(&store, &changed_files, &config);
            print!("{}", prediction::scoring::format_predictions(&predictions));
        }

        Commands::PrAnalysis { changed } => {
            let store = store::graph::GraphStore::new(conn)?;
            let changed_files: Vec<String> =
                changed.split(',').map(|s| s.trim().to_string()).collect();
            let config = integrations::github::PrAnalysisConfig::default();
            let report =
                integrations::github::analysis::analyze_pr(&store, &changed_files, &config);
            print!(
                "{}",
                integrations::github::analysis::format_report_markdown(&report)
            );
        }

        Commands::CiReport {
            changed,
            format,
            fail_on,
        } => {
            let store = store::graph::GraphStore::new(conn)?;
            let changed_files: Vec<String> =
                changed.split(',').map(|s| s.trim().to_string()).collect();
            let threshold: integrations::cicd::FailThreshold = fail_on.parse()?;
            let report =
                integrations::cicd::reporter::generate_report(&store, &changed_files, threshold);

            let output_format: integrations::cicd::OutputFormat = format.parse()?;
            match output_format {
                integrations::cicd::OutputFormat::Sarif => {
                    let sarif = integrations::cicd::sarif::to_sarif(&report);
                    println!("{}", serde_json::to_string_pretty(&sarif)?);
                }
                integrations::cicd::OutputFormat::GithubActions => {
                    print!(
                        "{}",
                        integrations::cicd::github_actions::format_annotations(&report)
                    );
                }
                integrations::cicd::OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                }
            }

            std::process::exit(report.exit_code);
        }

        Commands::Coverage { subcmd } => match subcmd {
            CoverageCommands::Import { file, format } => {
                let content = std::fs::read_to_string(&file)?;
                let fmt = match format.as_deref() {
                    Some("lcov") => "lcov",
                    Some("json") => "json",
                    Some(other) => anyhow::bail!("Unknown coverage format: {}", other),
                    None => coverage::parser::detect_format(&content)?,
                };

                let files = match fmt {
                    "lcov" => coverage::parser::parse_lcov(&content)?,
                    "json" => coverage::parser::parse_json(&content)?,
                    _ => unreachable!(),
                };

                coverage::store::init_coverage_table(&conn)?;
                let count = coverage::store::write_coverage(&conn, &files)?;
                println!("Imported coverage for {} files.", count);
            }
            CoverageCommands::Report => {
                coverage::store::init_coverage_table(&conn)?;
                let report = coverage::store::read_all_coverage(&conn)?;
                print!("{}", coverage::overlay::format_coverage_report(&report));
            }
            CoverageCommands::Gaps {
                min_connections,
                limit,
            } => {
                let store = store::graph::GraphStore::new(conn)?;
                let cov_conn = rusqlite::Connection::open(&cli.db)?;
                coverage::store::init_coverage_table(&cov_conn)?;
                let gaps = coverage::overlay::find_coverage_gaps(
                    &store,
                    &cov_conn,
                    min_connections,
                    limit,
                )?;
                print!("{}", coverage::overlay::format_coverage_gaps(&gaps));
            }
        },

        Commands::Policy { subcmd } => match subcmd {
            PolicyCommands::Check { config } => {
                let store = store::graph::GraphStore::new(conn)?;
                let yaml = std::fs::read_to_string(&config)?;
                let policy_config = policy::rules::parse_policy_config(&yaml)?;
                let result = policy::engine::evaluate(&store, &policy_config);
                print!("{}", policy::report::format_report(&result));

                if result.has_errors {
                    std::process::exit(1);
                }
            }
            PolicyCommands::Init => {
                print!("{}", policy::rules::generate_starter_config());
            }
        },
    }

    Ok(())
}
