use anyhow::Result;
use serde_json::Value;

use crate::prediction;
use crate::query;
use crate::store::graph::GraphStore;

pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

pub fn tool_definitions() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "cartograph_blast_radius",
            description: "Show all entities affected by changes to the given entity, up to a traversal depth",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "entity": {
                        "type": "string",
                        "description": "File path or entity name"
                    },
                    "depth": {
                        "type": "integer",
                        "description": "Max traversal depth (default: 3)"
                    }
                },
                "required": ["entity"]
            }),
        },
        ToolDef {
            name: "cartograph_dependencies",
            description: "Show direct dependencies of an entity (upstream or downstream)",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "entity": {
                        "type": "string",
                        "description": "File path or entity name"
                    },
                    "direction": {
                        "type": "string",
                        "description": "Direction: 'upstream' (what this depends on) or 'downstream' (what depends on this). Default: downstream"
                    }
                },
                "required": ["entity"]
            }),
        },
        ToolDef {
            name: "cartograph_co_changes",
            description: "Show files that historically co-change with the given entity (change together frequently)",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "entity": {
                        "type": "string",
                        "description": "File path or entity name"
                    }
                },
                "required": ["entity"]
            }),
        },
        ToolDef {
            name: "cartograph_who_owns",
            description: "Show who owns an entity based on git blame analysis",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "entity": {
                        "type": "string",
                        "description": "File path or entity name"
                    }
                },
                "required": ["entity"]
            }),
        },
        ToolDef {
            name: "cartograph_hotspots",
            description: "Show the most highly-connected change hotspots in the codebase",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "limit": {
                        "type": "integer",
                        "description": "Number of results to return (default: 20)"
                    }
                },
                "required": []
            }),
        },
        ToolDef {
            name: "cartograph_predict_risk",
            description: "Predict regression risk for files based on a set of changed files. Uses structural coupling, co-change frequency, hotspot centrality, and ownership fragmentation signals.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "changed_files": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "List of changed file paths to analyze"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results (default: 20, max: 200)"
                    }
                },
                "required": ["changed_files"]
            }),
        },
    ]
}

/// Maximum traversal depth for blast radius queries
const MAX_DEPTH: usize = 10;
/// Maximum results for hotspot queries
const MAX_LIMIT: usize = 500;
/// Maximum entity path length
const MAX_ENTITY_LEN: usize = 1024;

fn validate_entity(params: &Value) -> Result<&str> {
    let entity = params["entity"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing required param: entity"))?;
    if entity.len() > MAX_ENTITY_LEN {
        anyhow::bail!("entity path too long (max {} chars)", MAX_ENTITY_LEN);
    }
    if entity.contains("..") {
        anyhow::bail!("entity path must not contain '..'");
    }
    Ok(entity)
}

pub fn execute_tool(store: &GraphStore, name: &str, params: &Value) -> Result<String> {
    match name {
        "cartograph_blast_radius" => {
            let entity = validate_entity(params)?;
            let depth = (params["depth"].as_u64().unwrap_or(3) as usize).min(MAX_DEPTH);

            let results = query::blast_radius::query(store, entity, depth);

            if results.is_empty() {
                return Ok(format!("No blast radius results for '{entity}'"));
            }

            let mut out = format!("{:<40} {:<10} {}\n", "ENTITY", "DEPTH", "EDGE");
            out.push_str(&"-".repeat(60));
            out.push('\n');
            for r in &results {
                let path = r.entity_path.as_deref().unwrap_or(&r.entity_name);
                out.push_str(&format!("{:<40} {:<10} {}\n", path, r.depth, r.edge_kind));
            }
            Ok(out)
        }

        "cartograph_dependencies" => {
            let entity = validate_entity(params)?;
            let direction = params["direction"].as_str().unwrap_or("downstream");

            let dir = match direction {
                "upstream" => petgraph::Direction::Incoming,
                _ => petgraph::Direction::Outgoing,
            };

            let Some(e) = store.find_entity_by_path(entity) else {
                return Ok(format!("Entity not found: {entity}"));
            };

            let deps = store.dependencies(&e.id, dir);
            if deps.is_empty() {
                return Ok(format!("No {direction} dependencies for '{entity}'"));
            }

            let mut out = format!("{:<40} {}\n", "ENTITY", "KIND");
            out.push_str(&"-".repeat(50));
            out.push('\n');
            for d in &deps {
                let path = d.path.as_deref().unwrap_or(&d.name);
                out.push_str(&format!("{:<40} {:?}\n", path, d.kind));
            }
            Ok(out)
        }

        "cartograph_co_changes" => {
            let entity = validate_entity(params)?;

            let results = query::co_changes(store, entity);

            if results.is_empty() {
                return Ok(format!("No co-change data for '{entity}'"));
            }

            let mut out = format!("{:<40} {}\n", "ENTITY", "CONFIDENCE");
            out.push_str(&"-".repeat(55));
            out.push('\n');
            for r in &results {
                let path = r.entity_path.as_deref().unwrap_or(&r.entity_name);
                out.push_str(&format!("{:<40} {:.2}\n", path, r.confidence));
            }
            Ok(out)
        }

        "cartograph_who_owns" => {
            let entity = validate_entity(params)?;

            let results = query::ownership::query(store, entity);

            if results.is_empty() {
                return Ok(format!("No ownership data for '{entity}'"));
            }

            let mut out = format!("{:<30} {}\n", "OWNER", "CONFIDENCE");
            out.push_str(&"-".repeat(45));
            out.push('\n');
            for r in &results {
                out.push_str(&format!("{:<30} {:.2}\n", r.entity_name, r.confidence));
            }
            Ok(out)
        }

        "cartograph_hotspots" => {
            let limit = (params["limit"].as_u64().unwrap_or(20) as usize).min(MAX_LIMIT);

            let results = query::hotspots::query(store, limit);

            if results.is_empty() {
                return Ok("No hotspot data found. Run 'index' first.".to_string());
            }

            let mut out = format!("{:<40} {}\n", "ENTITY", "CONNECTIONS");
            out.push_str(&"-".repeat(55));
            out.push('\n');
            for r in &results {
                let path = r.entity_path.as_deref().unwrap_or(&r.entity_name);
                out.push_str(&format!("{:<40} {}\n", path, r.edge_count));
            }
            Ok(out)
        }

        "cartograph_predict_risk" => {
            let changed_files: Vec<String> = params["changed_files"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("missing required param: changed_files"))?
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();

            let limit = (params["limit"].as_u64().unwrap_or(20) as usize).min(MAX_LIMIT);

            let config = prediction::PredictionConfig {
                max_results: limit,
                ..prediction::PredictionConfig::default()
            };

            match prediction::scoring::predict_regressions(store, &changed_files, &config) {
                Ok(predictions) => Ok(prediction::scoring::format_predictions(&predictions)),
                Err(e) => Err(anyhow::anyhow!("{}", e)),
            }
        }

        other => Err(anyhow::anyhow!("Unknown tool: {other}")),
    }
}
