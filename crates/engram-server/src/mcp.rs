//! Model Context Protocol (MCP) server — JSON-RPC 2.0 over stdio.
//!
//! Implements the MCP specification (protocol version 2024-11-05).
//! Communicates over stdin/stdout, one JSON object per line.
//!
//! # Tools exposed to the LLM
//!
//! | Tool | Arguments | Description |
//! |------|-----------|-------------|
//! | `remember` | `concept: str, text: str` | Encode text and store as a memory |
//! | `recall` | `query: str, k: int` | Find k most similar memories |
//! | `forget` | `concept: str` | Delete a memory |
//! | `list_concepts` | — | List all stored concept names |
//!
//! # Claude Desktop config
//!
//! ```json
//! {
//!   "mcpServers": {
//!     "engram": {
//!       "command": "/path/to/engram",
//!       "args": ["mcp", "--store", "~/.engram/manifold"]
//!     }
//!   }
//! }
//! ```

use crate::store::SharedStore;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use tracing::{debug, error, info, warn};

// ── JSON-RPC 2.0 types ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct Request {
    #[allow(dead_code)]
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct Response {
    jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcError>,
}

#[derive(Debug, Serialize)]
struct RpcError {
    code: i32,
    message: String,
}

impl Response {
    fn ok(id: Option<Value>, result: Value) -> Self {
        Self { jsonrpc: "2.0", id, result: Some(result), error: None }
    }
    fn err(id: Option<Value>, code: i32, message: impl Into<String>) -> Self {
        Self { jsonrpc: "2.0", id, result: None, error: Some(RpcError { code, message: message.into() }) }
    }
}

// ── MCP tool definitions ──────────────────────────────────────────────────────

fn tool_list() -> Value {
    json!({
        "tools": [
            {
                "name": "remember",
                "description": "Encode text and store it as a persistent memory under a concept name. \
                                Use this to save facts, context, or information for later retrieval.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "concept": {
                            "type": "string",
                            "description": "A unique identifier for this memory (e.g. 'krebs_cycle', 'user_preference_dark_mode')"
                        },
                        "text": {
                            "type": "string",
                            "description": "The text content to encode and store"
                        }
                    },
                    "required": ["concept", "text"]
                }
            },
            {
                "name": "recall",
                "description": "Search persistent memory by semantic similarity. Returns the k most relevant \
                                memories for the given query. Use this to retrieve context before answering questions.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Natural language query describing what you want to find"
                        },
                        "k": {
                            "type": "integer",
                            "description": "Number of results to return (default: 5, max: 20)",
                            "default": 5
                        }
                    },
                    "required": ["query"]
                }
            },
            {
                "name": "forget",
                "description": "Delete a specific memory by concept name.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "concept": {
                            "type": "string",
                            "description": "The concept name to delete"
                        }
                    },
                    "required": ["concept"]
                }
            },
            {
                "name": "list_concepts",
                "description": "List all concept names currently stored in memory.",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            },
            {
                "name": "mcp_engram_watch_workspace",
                "description": "Tell the background Agentic Daemon to recursively watch a specific directory for native file-saves.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Absolute path to the workspace folder (e.g. /home/a/Documents/CodeLand)"
                        }
                    },
                    "required": ["path"]
                }
            },
            {
                "name": "mcp_engram_pin",
                "description": "Lock a project management concept into the manifold so the Autophagy Daemon never decays it.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "concept": {
                            "type": "string",
                            "description": "Concept tag to pin (e.g. 'task_board' or 'system_architecture')"
                        }
                    },
                    "required": ["concept"]
                }
            },
            {
                "name": "mcp_engram_relate",
                "description": "Bind two concepts via op_bind and store a directional relation block (ZEDOS_RELATION).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "concept_a": {
                            "type": "string",
                            "description": "Source concept"
                        },
                        "concept_b": {
                            "type": "string",
                            "description": "Target concept"
                        },
                        "label": {
                            "type": "string",
                            "description": "Relation label (e.g. 'depends_on', 'implements')"
                        }
                    },
                    "required": ["concept_a", "concept_b", "label"]
                }
            },
            {
                "name": "mcp_engram_context_for_file",
                "description": "Surface the top 5 most relevant memories for a given file path. Useful for proactive context loading when opening a file.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "File path (e.g. /home/user/project/backend.rs)"
                        }
                    },
                    "required": ["path"]
                }
            },
            {
                "name": "mcp_engram_remember_solution",
                "description": "Store a crystallized error→solution pair as a ZEDOS_PRAXIS block, auto-pinned to CRS=1.0. Solutions never decay.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "error_pattern": {
                            "type": "string",
                            "description": "The error or problem pattern (error message, concept, or description)"
                        },
                        "solution": {
                            "type": "string",
                            "description": "The solution or approach that resolved it"
                        }
                    },
                    "required": ["error_pattern", "solution"]
                }
            },
            {
                "name": "mcp_engram_stats",
                "description": "Return a health report of the geometric manifold: total memories, pinned count, CRS distribution, active namespace, and disk usage.",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "mcp_engram_recall_recent",
                "description": "Return the N most recently accessed memories, sorted by access time. Useful for session rehydration when you don't know exact concept names.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "n": {
                            "type": "integer",
                            "description": "Number of recent memories to return (default: 10)",
                            "default": 10
                        }
                    }
                }
            },
            {
                "name": "mcp_engram_set_namespace",
                "description": "Switch to a project-specific memory namespace (stalk). Creates the namespace if it does not exist. Use this to isolate memories by project.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "namespace": {
                            "type": "string",
                            "description": "Namespace name (e.g. 'codeland', 'personal', 'work_project_x')"
                        }
                    },
                    "required": ["namespace"]
                }
            },
            {
                "name": "mcp_engram_list_namespaces",
                "description": "List all available memory namespaces and which one is currently active.",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "mcp_engram_update",
                "description": "Update the text of an existing memory in place. Re-encodes the vector and bumps CRS. Use when context changes and the old memory is stale.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "concept": {
                            "type": "string",
                            "description": "The concept name to update"
                        },
                        "new_text": {
                            "type": "string",
                            "description": "The new text content to encode"
                        }
                    },
                    "required": ["concept", "new_text"]
                }
            },
            {
                "name": "mcp_engram_summarize",
                "description": "Return a project-state digest: all pinned memories first, then the top N memories by CRS score. Ideal as a single-call /wake_up replacement.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "top_n": {
                            "type": "integer",
                            "description": "How many non-pinned memories to include, sorted by CRS (default: 10)",
                            "default": 10
                        }
                    }
                }
            },
            {
                "name": "mcp_engram_batch_remember",
                "description": "Store multiple memories in a single call. Faster than calling remember() N times.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "entries": {
                            "type": "array",
                            "description": "Array of {concept, text} objects to store",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "concept": { "type": "string" },
                                    "text":    { "type": "string" }
                                },
                                "required": ["concept", "text"]
                            }
                        }
                    },
                    "required": ["entries"]
                }
            },
            {
                "name": "mcp_engram_export",
                "description": "Export the manifold (or a filtered subset) as a portable JSON array. Use for backup, migration, or cross-machine sync.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "min_crs": {
                            "type": "number",
                            "description": "Only export memories with CRS >= this value (default: 0.0 = all)",
                            "default": 0.0
                        }
                    }
                }
            },
            {
                "name": "mcp_engram_import",
                "description": "Restore memories from a JSON array previously exported by mcp_engram_export. Each entry needs concept and text fields.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "json": {
                            "type": "string",
                            "description": "JSON string: array of {concept, text} objects"
                        }
                    },
                    "required": ["json"]
                }
            },
            {
                "name": "mcp_engram_forget_old",
                "description": "Manually trigger autophagy: evict all memories below a CRS threshold (pinned memories are always exempt).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "min_crs_threshold": {
                            "type": "number",
                            "description": "Evict memories with CRS below this value (default: 0.2)",
                            "default": 0.2
                        },
                        "older_than_days": {
                            "type": "integer",
                            "description": "If set, only evict memories not accessed in this many days"
                        }
                    }
                }
            },
            {
                "name": "mcp_engram_search_by_relation",
                "description": "Traverse the knowledge graph. Find all concepts related to a seed concept, filtered by optional label and direction.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "concept": {
                            "type": "string",
                            "description": "The seed concept to query"
                        },
                        "label": {
                            "type": "string",
                            "description": "Optional: filter by relation label (e.g. 'depends_on', 'implements')"
                        },
                        "direction": {
                            "type": "string",
                            "description": "'from' (A→?), 'to' (?→A), or 'both' (default: 'from')",
                            "enum": ["from", "to", "both"],
                            "default": "from"
                        }
                    },
                    "required": ["concept"]
                }
            },
            {
                "name": "mcp_engram_visualize",
                "description": "Render a BFS subgraph from a seed concept as a Mermaid diagram. Shows how concepts are related to each other.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "concept": {
                            "type": "string",
                            "description": "The seed concept to start the graph from"
                        },
                        "depth": {
                            "type": "integer",
                            "description": "BFS depth (default: 2, max: 5)",
                            "default": 2
                        }
                    },
                    "required": ["concept"]
                }
            },
            {
                "name": "mcp_engram_genesis",
                "description": "Inspect or re-seed the alignment genesis blocks. Genesis seeds are PRAXIS-tagged memories at CRS=1.0 that anchor the manifold's ethical and operational context. They are seeded once on first boot and never decay.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "description": "'status' — show which genesis blocks exist. 'reseed' — re-seed all blocks.",
                            "enum": ["status", "reseed"],
                            "default": "status"
                        }
                    }
                }
            }
        ]
    })
}

// ── Tool dispatch ─────────────────────────────────────────────────────────────

fn handle_tool_call(name: &str, args: &Value, store: &SharedStore) -> Value {
    match name {
        "remember" => {
            let concept = args["concept"].as_str().unwrap_or("").trim().to_string();
            let text    = args["text"].as_str().unwrap_or("").trim().to_string();

            if concept.is_empty() || text.is_empty() {
                return json!({
                    "content": [{ "type": "text", "text": "Error: concept and text are required." }],
                    "isError": true
                });
            }
            match store.lock().unwrap().remember(&concept, &text) {
                Ok(_) => {
                    info!("remembered: {concept}");
                    json!({
                        "content": [{
                            "type": "text",
                            "text": format!("✓ Stored memory: '{concept}' ({} chars)", text.len())
                        }]
                    })
                }
                Err(e) => json!({
                    "content": [{ "type": "text", "text": format!("Error storing memory: {e}") }],
                    "isError": true
                }),
            }
        }

        "recall" => {
            let query = args["query"].as_str().unwrap_or("").trim().to_string();
            let k = args["k"].as_u64().unwrap_or(5).min(20) as usize;

            if query.is_empty() {
                return json!({
                    "content": [{ "type": "text", "text": "Error: query is required." }],
                    "isError": true
                });
            }

            let results = store.lock().unwrap().recall(&query, k);
            if results.is_empty() {
                return json!({
                    "content": [{ "type": "text", "text": "No memories found." }]
                });
            }

            let mut output = format!("Found {} memories:\n\n", results.len());
            for (i, mem) in results.iter().enumerate() {
                output.push_str(&format!(
                    "**[{}] {}** (similarity: {:.3}, crs: {:.3})\n{}\n\n",
                    i + 1, mem.concept, mem.score, mem.crs,
                    if mem.provlog.is_empty() { "(no text content)" } else { mem.provlog.as_str() }
                ));
            }

            debug!("recall '{}' → {} results", query, results.len());
            json!({ "content": [{ "type": "text", "text": output.trim() }] })
        }

        "forget" => {
            let concept = args["concept"].as_str().unwrap_or("").trim().to_string();
            if concept.is_empty() {
                return json!({
                    "content": [{ "type": "text", "text": "Error: concept is required." }],
                    "isError": true
                });
            }
            match store.lock().unwrap().forget(&concept) {
                Ok(_) => {
                    info!("forgot: {concept}");
                    json!({ "content": [{ "type": "text", "text": format!("✓ Deleted memory: '{concept}'") }] })
                }
                Err(e) => json!({
                    "content": [{ "type": "text", "text": format!("Error: {e}") }],
                    "isError": true
                }),
            }
        }

        "list_concepts" => {
            let concepts = store.lock().unwrap().list();
            if concepts.is_empty() {
                json!({ "content": [{ "type": "text", "text": "No memories stored yet." }] })
            } else {
                let list = concepts.iter()
                    .map(|c| format!("  • {c}"))
                    .collect::<Vec<_>>()
                    .join("\n");
                json!({ "content": [{ "type": "text", "text": format!("{} memories:\n{}", concepts.len(), list) }] })
            }
        }

        "mcp_engram_watch_workspace" => {
            let path = args["path"].as_str().unwrap_or("").trim().to_string();
            let lock = store.lock().unwrap();
            if let Some(daemon) = &lock.daemon {
                let d = daemon.clone();
                let p = path.clone();
                tokio::spawn(async move { d.set_watch_workspace(&p).await; });
            }
            json!({
                "content": [{ "type": "text", "text": format!("✓ Agentic Daemon now recursively watching: {}", path) }]
            })
        }

        "mcp_engram_pin" => {
            let concept = args["concept"].as_str().unwrap_or("").trim().to_string();
            let mut lock = store.lock().unwrap();
            if let Some(mut m) = lock.fetch_block(&concept) {
                m.crs_score = 1.0; // Pinned mathematically
                let _ = lock.store(&concept, m);
                json!({ "content": [{ "type": "text", "text": format!("✓ Pinned concept to CRS 1.0. Autophagy will ignore it.: {}", concept) }] })
            } else {
                json!({ "content": [{ "type": "text", "text": format!("Memory not found: {}", concept) }], "isError": true })
            }
        }

        "mcp_engram_relate" => {
            let concept_a = args["concept_a"].as_str().unwrap_or("").trim().to_string();
            let concept_b = args["concept_b"].as_str().unwrap_or("").trim().to_string();
            let label     = args["label"].as_str().unwrap_or("").trim().to_string();

            if concept_a.is_empty() || concept_b.is_empty() || label.is_empty() {
                return json!({ "content": [{ "type": "text", "text": "Error: missing required strings" }], "isError": true });
            }

            // Strip sheaf prefix if present, since relate() uses fetch_block internally
            let raw_a = concept_a.split_once("::").map_or(concept_a.as_str(), |(_, r)| r);
            let raw_b = concept_b.split_once("::").map_or(concept_b.as_str(), |(_, r)| r);

            match store.lock().unwrap().relate(raw_a, raw_b, &label) {
                Ok(msg) => json!({ "content": [{ "type": "text", "text": msg }] }),
                Err(e)  => json!({ "content": [{ "type": "text", "text": format!("Error adding relation: {e}") }], "isError": true }),
            }
        }

        "mcp_engram_context_for_file" => {
            let path = args["path"].as_str().unwrap_or("").trim().to_string();
            if path.is_empty() {
                return json!({ "content": [{ "type": "text", "text": "Error: path is required." }], "isError": true });
            }

            // Use the dedicated context_for_file method which enriches queries with language context
            let results = store.lock().unwrap().context_for_file(&path);
            if results.is_empty() {
                return json!({ "content": [{ "type": "text", "text": format!("No specific topological memory found for {}", path) }] });
            }

            let mut output = format!("Architectural Context for {}:\n\n", path);
            for mem in results.iter() {
                output.push_str(&format!(
                    "**{}** (crs: {:.2})\n{}\n\n",
                    mem.concept, mem.crs,
                    if mem.provlog.is_empty() { "(no text content)" } else { mem.provlog.as_str() }
                ));
            }
            json!({ "content": [{ "type": "text", "text": output.trim() }] })
        }

        "mcp_engram_remember_solution" => {
            let error_pattern = args["error_pattern"].as_str().unwrap_or("").trim().to_string();
            let solution      = args["solution"].as_str().unwrap_or("").trim().to_string();
            
            if error_pattern.is_empty() || solution.is_empty() {
                return json!({ "content": [{ "type": "text", "text": "Error: missing required strings" }], "isError": true });
            }

            // Synthesize the concept name securely
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            error_pattern.hash(&mut h);
            let concept_name = format!("praxis_solution_{}", h.finish());
            let payload = format!("ERROR PATTERN:\n{}\n\nSOLUTION:\n{}", error_pattern, solution);

            let mut lock = store.lock().unwrap();
            match lock.remember(&concept_name, &payload) {
                Ok(_) => {
                    // Fetch the block immediately to pin and tag it
                    if let Some(mut m) = lock.fetch_block(&concept_name) {
                        m.zedos_tag = engram_core::types::ZEDOS_PRAXIS;
                        m.crs_score = 1.0; // Pinned mathematically
                        let _ = lock.store(&concept_name, m);
                    }
                    json!({ "content": [{ "type": "text", "text": format!("✓ Crystallized Solution permanently into geometric memory (CRS = 1.0).\nStored as: {}", concept_name) }] })
                }
                Err(e) => json!({ "content": [{ "type": "text", "text": format!("Failed to crystallize solution: {}", e) }], "isError": true })
            }
        }

        "mcp_engram_stats" => {
            let lock = store.lock().unwrap();
            let concepts = lock.list();
            let total = concepts.len();
            let mut pinned = 0usize;
            let mut crs_sum = 0.0f32;
            let mut crs_min = f32::MAX;
            let mut crs_max = 0.0f32;
            for name in &concepts {
                if let Some(block) = lock.fetch_block(name.split_once("::").map_or(name.as_str(), |(_, r)| r)) {
                    let crs = block.crs_score;
                    if crs >= 1.0 { pinned += 1; }
                    crs_sum += crs;
                    if crs < crs_min { crs_min = crs; }
                    if crs > crs_max { crs_max = crs; }
                }
            }
            let avg_crs = if total > 0 { crs_sum / total as f32 } else { 0.0 };
            let path = lock.store_path().to_string();
            let active_ns = lock.active_stalk_name();
            drop(lock);

            let disk_kb = std::fs::read_dir(&path)
                .map(|entries| entries
                    .filter_map(|e| e.ok())
                    .filter_map(|e| e.metadata().ok())
                    .map(|m| m.len())
                    .sum::<u64>())
                .unwrap_or(0) as f64 / 1024.0;

            let report = format!(
                "📊 Engram Manifold Stats\n\
                 ─────────────────────────\n\
                 Total Memories : {total}\n\
                 Pinned (CRS=1.0): {pinned}\n\
                 Avg CRS        : {avg_crs:.3}\n\
                 Min CRS        : {:.3}\n\
                 Max CRS        : {crs_max:.3}\n\
                 Active NS      : {active_ns}\n\
                 Disk Usage     : {disk_kb:.1} KB\n\
                 Store Path     : {path}",
                if total > 0 { crs_min } else { 0.0 }
            );
            json!({ "content": [{ "type": "text", "text": report }] })
        }

        "mcp_engram_recall_recent" => {
            let n = args["n"].as_u64().unwrap_or(10).min(50) as usize;
            let recent = store.lock().unwrap().recent(n);
            if recent.is_empty() {
                return json!({ "content": [{ "type": "text", "text": "No memories accessed yet." }] });
            }
            let mut out = format!("🕐 {} Most Recently Accessed Memories:\n\n", recent.len());
            for (i, (concept, ts)) in recent.iter().enumerate() {
                // Convert unix seconds to a readable relative label
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let age_secs = now.saturating_sub(*ts);
                let age = if age_secs < 60 { format!("{age_secs}s ago") }
                    else if age_secs < 3600 { format!("{}m ago", age_secs / 60) }
                    else if age_secs < 86400 { format!("{}h ago", age_secs / 3600) }
                    else { format!("{}d ago", age_secs / 86400) };
                out.push_str(&format!("  {}. {} ({})", i + 1, concept, age));
                out.push('\n');
            }
            info!("recall_recent → {} results", recent.len());
            json!({ "content": [{ "type": "text", "text": out.trim() }] })
        }

        "mcp_engram_set_namespace" => {
            let namespace = args["namespace"].as_str().unwrap_or("").trim().to_string();
            if namespace.is_empty() {
                return json!({ "content": [{ "type": "text", "text": "Error: namespace is required." }], "isError": true });
            }
            let lock = store.lock().unwrap();
            let is_sheaf = lock.is_sheaf_mode();
            let ok = lock.set_active_stalk(&namespace);
            if ok {
                info!("namespace set to: {namespace}");
                json!({ "content": [{ "type": "text", "text": format!("✓ Active namespace set to '{namespace}'") }] })
            } else if !is_sheaf {
                json!({ "content": [{ "type": "text", "text": "Namespaces require sheaf mode. Create ~/.engram/sheaf.toml to enable multi-project namespaces." }], "isError": true })
            } else {
                json!({ "content": [{ "type": "text", "text": format!("Namespace '{namespace}' not found in sheaf.toml. Add it to your stalk configuration.") }], "isError": true })
            }
        }

        "mcp_engram_list_namespaces" => {
            let lock = store.lock().unwrap();
            let namespaces = lock.stalk_names();
            let active = lock.active_stalk_name();
            drop(lock);
            if namespaces.is_empty() {
                json!({ "content": [{ "type": "text", "text": "Only the default namespace exists." }] })
            } else {
                let list = namespaces.iter()
                    .map(|ns| if ns == &active { format!("  • {} ← active", ns) } else { format!("  • {}", ns) })
                    .collect::<Vec<_>>()
                    .join("\n");
                json!({ "content": [{ "type": "text", "text": format!("Namespaces ({}):\n{}", namespaces.len(), list) }] })
            }
        }

        "mcp_engram_update" => {
            let concept  = args["concept"].as_str().unwrap_or("").trim().to_string();
            let new_text = args["new_text"].as_str().unwrap_or("").trim().to_string();
            if concept.is_empty() || new_text.is_empty() {
                return json!({ "content": [{ "type": "text", "text": "Error: concept and new_text are required." }], "isError": true });
            }
            match store.lock().unwrap().update(&concept, &new_text) {
                Ok(status) => {
                    info!("updated: {concept}");
                    json!({ "content": [{ "type": "text", "text": format!("✓ Updated memory '{concept}': {status}") }] })
                }
                Err(e) => json!({ "content": [{ "type": "text", "text": format!("Error updating '{concept}': {e}") }], "isError": true })
            }
        }

        "mcp_engram_summarize" => {
            let top_n = args["top_n"].as_u64().unwrap_or(10).min(50) as usize;
            let lock = store.lock().unwrap();
            let concepts = lock.list();
            let mut pinned: Vec<(String, f32, String)> = Vec::new();
            let mut ranked: Vec<(String, f32, String)> = Vec::new();

            for name in &concepts {
                if let Some(block) = lock.fetch_block(name.split_once("::").map_or(name.as_str(), |(_, r)| r)) {
                    let crs = block.crs_score;
                    let raw = String::from_utf8_lossy(&block.payload);
                    let text = raw.trim_matches('\0');
                    let preview: String = text.chars().take(120).collect();
                    let preview = if text.len() > 120 { format!("{}...", preview) } else { preview.to_string() };
                    if crs >= 1.0 {
                        pinned.push((name.clone(), crs, preview));
                    } else {
                        ranked.push((name.clone(), crs, preview));
                    }
                }
            }
            drop(lock);

            ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            ranked.truncate(top_n);

            let mut out = String::from("\u{1f4cb} Engram Project Summary\n\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\n");
            if !pinned.is_empty() {
                out.push_str(&format!("\n\u{1f4cc} PINNED ({}):\n", pinned.len()));
                for (i, (name, crs, preview)) in pinned.iter().enumerate() {
                    out.push_str(&format!("  {}. {} [CRS {:.3}]\n     {}\n\n", i + 1, name, crs, preview));
                }
            }
            if !ranked.is_empty() {
                out.push_str(&format!("\u{1f9e0} TOP {} BY CRS:\n", ranked.len()));
                for (i, (name, crs, preview)) in ranked.iter().enumerate() {
                    out.push_str(&format!("  {}. {} [CRS {:.3}]\n     {}\n\n", i + 1, name, crs, preview));
                }
            }
            if pinned.is_empty() && ranked.is_empty() {
                out.push_str("No memories stored yet.");
            }
            info!("summarize: {} pinned, {} ranked", pinned.len(), ranked.len());
            json!({ "content": [{ "type": "text", "text": out.trim() }] })
        }

        "mcp_engram_batch_remember" => {
            let entries = match args["entries"].as_array() {
                Some(a) => a.clone(),
                None => return json!({ "content": [{ "type": "text", "text": "Error: entries must be a JSON array of {concept, text} objects." }], "isError": true }),
            };
            let mut succeeded = 0usize;
            let mut failed = 0usize;
            for entry in &entries {
                let concept = entry["concept"].as_str().unwrap_or("").trim().to_string();
                let text    = entry["text"].as_str().unwrap_or("").trim().to_string();
                if concept.is_empty() || text.is_empty() { failed += 1; continue; }
                match store.lock().unwrap().remember(&concept, &text) {
                    Ok(_)  => succeeded += 1,
                    Err(_) => failed += 1,
                }
            }
            info!("batch_remember: {} ok, {} failed", succeeded, failed);
            json!({ "content": [{ "type": "text", "text": format!("\u{2713} Batch ingestion complete: {} stored, {} failed.", succeeded, failed) }] })
        }

        "mcp_engram_export" => {
            let min_crs = args["min_crs"].as_f64().unwrap_or(0.0) as f32;
            let lock = store.lock().unwrap();
            let concepts = lock.list();
            let mut exported: Vec<Value> = Vec::new();
            for name in &concepts {
                if let Some(block) = lock.fetch_block(name.split_once("::").map_or(name.as_str(), |(_, r)| r)) {
                    if block.crs_score < min_crs { continue; }
                    let raw = String::from_utf8_lossy(&block.payload);
                    let text = raw.trim_matches('\0').to_string();
                    exported.push(json!({
                        "concept": name,
                        "text": text,
                        "crs": block.crs_score,
                        "zedos_tag": block.zedos_tag,
                        "last_accessed": block.last_accessed_timestamp
                    }));
                }
            }
            drop(lock);
            let count = exported.len();
            let json_str = serde_json::to_string_pretty(&exported).unwrap_or_default();
            info!("export: {} memories", count);
            json!({ "content": [{ "type": "text", "text": format!("Exported {} memories:\n```json\n{}\n```", count, json_str) }] })
        }

        "mcp_engram_import" => {
            let json_str = args["json"].as_str().unwrap_or("").trim().to_string();
            if json_str.is_empty() {
                return json!({ "content": [{ "type": "text", "text": "Error: json field is required." }], "isError": true });
            }
            let entries: Vec<Value> = match serde_json::from_str(&json_str) {
                Ok(v)  => v,
                Err(e) => return json!({ "content": [{ "type": "text", "text": format!("Error parsing JSON: {e}") }], "isError": true }),
            };
            let mut succeeded = 0usize;
            let mut failed = 0usize;
            for entry in &entries {
                let concept = entry["concept"].as_str().unwrap_or("").trim().to_string();
                let text    = entry["text"].as_str().unwrap_or("").trim().to_string();
                if concept.is_empty() || text.is_empty() { failed += 1; continue; }
                match store.lock().unwrap().remember(&concept, &text) {
                    Ok(_)  => succeeded += 1,
                    Err(_) => failed += 1,
                }
            }
            info!("import: {} ok, {} failed", succeeded, failed);
            json!({ "content": [{ "type": "text", "text": format!("\u{2713} Import complete: {} memories restored, {} failed.", succeeded, failed) }] })
        }

        "mcp_engram_forget_old" => {
            let min_crs = args["min_crs_threshold"].as_f64().unwrap_or(0.2) as f32;
            let older_than_days = args["older_than_days"].as_u64();
            let now_secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs()).unwrap_or(0);

            let lock = store.lock().unwrap();
            let concepts = lock.list();
            let mut to_evict: Vec<String> = Vec::new();
            for name in &concepts {
                if let Some(block) = lock.fetch_block(name.split_once("::").map_or(name.as_str(), |(_, r)| r)) {
                    if block.crs_score >= 1.0 { continue; } // Never evict pinned
                    let age_ok = older_than_days.is_none_or(|days| {
                        now_secs.saturating_sub(block.last_accessed_timestamp) >= days * 86400
                    });
                    if block.crs_score < min_crs && age_ok {
                        to_evict.push(name.split_once("::").map_or(name.as_str(), |(_, r)| r).to_string());
                    }
                }
            }
            drop(lock);

            let total = to_evict.len();
            let mut evicted = 0usize;
            for name in &to_evict {
                if store.lock().unwrap().forget(name).is_ok() { evicted += 1; }
            }
            let age_label = older_than_days.map_or(String::new(), |d| format!(", older than {}d", d));
            info!("forget_old: evicted {}/{} candidates", evicted, total);
            json!({ "content": [{ "type": "text", "text": format!("\u{2713} Autophagy complete. Evicted {} memories (CRS < {:.2}{}).", evicted, min_crs, age_label) }] })
        }

        "mcp_engram_search_by_relation" => {
            let concept   = args["concept"].as_str().unwrap_or("").trim().to_string();
            let label     = args["label"].as_str().map(|s| s.trim().to_string());
            let direction = args["direction"].as_str().unwrap_or("from").trim().to_string();

            if concept.is_empty() {
                return json!({ "content": [{ "type": "text", "text": "Error: concept is required." }], "isError": true });
            }

            let results = store.lock().unwrap()
                .search_relations(&concept, label.as_deref(), &direction);

            if results.is_empty() {
                let label_str = label.as_deref().unwrap_or("any");
                return json!({ "content": [{ "type": "text", "text": format!("No '{}' relations found for '{}' (direction: {}).", label_str, concept, direction) }] });
            }

            let arrow = match direction.as_str() { "to" => "→", _ => "→" };
            let mut out = format!("🕸️  Relations for '{}' (direction: {}):\n\n", concept, direction);
            for (lbl, other) in &results {
                match direction.as_str() {
                    "to" => out.push_str(&format!("  {} --[{}]--> {}\n", other, lbl, concept)),
                    _    => out.push_str(&format!("  {} --[{}]--> {}\n", concept, lbl, other)),
                }
            }
            let _ = arrow;
            info!("search_by_relation '{}' {} {} -> {} results", concept, direction, label.as_deref().unwrap_or("*"), results.len());
            json!({ "content": [{ "type": "text", "text": out.trim() }] })
        }

        "mcp_engram_visualize" => {
            let concept = args["concept"].as_str().unwrap_or("").trim().to_string();
            let depth   = args["depth"].as_u64().unwrap_or(2).min(5) as usize;

            if concept.is_empty() {
                return json!({ "content": [{ "type": "text", "text": "Error: concept is required." }], "isError": true });
            }

            let mermaid = store.lock().unwrap().visualize_graph(&concept, depth);
            info!("visualize '{}' depth {}", concept, depth);
            json!({ "content": [{ "type": "text", "text": mermaid }] })
        }

        "mcp_engram_genesis" => {
            let action = args["action"].as_str().unwrap_or("status").trim().to_string();
            match action.as_str() {
                "status" => {
                    let status = store.lock().unwrap().genesis_status();
                    info!("genesis status requested");
                    json!({ "content": [{ "type": "text", "text": status }] })
                }
                "reseed" => {
                    // Remove the marker so seed_genesis() runs again
                    let engram_root = std::path::PathBuf::from(
                        shellexpand::tilde("~/.engram").into_owned()
                    );
                    let marker = engram_root.join(".genesis_seeded");
                    let _ = std::fs::remove_file(&marker);
                    match store.lock().unwrap().seed_genesis() {
                        Ok(msg)  => { info!("genesis reseed: {msg}"); json!({ "content": [{ "type": "text", "text": msg }] }) }
                        Err(e)   => json!({ "content": [{ "type": "text", "text": format!("Genesis reseed failed: {e}") }], "isError": true })
                    }
                }
                _ => json!({ "content": [{ "type": "text", "text": "Unknown action. Use 'status' or 'reseed'." }], "isError": true })
            }
        }

        unknown => json!({
            "content": [{ "type": "text", "text": format!("Unknown tool: {unknown}") }],
            "isError": true
        }),
    }
}

// ── MCP request dispatch ──────────────────────────────────────────────────────

fn dispatch(req: Request, store: &SharedStore) -> Option<Response> {
    let id = req.id.clone();
    let params = req.params.unwrap_or(json!({}));

    // If ID is completely missing, it is a JSON-RPC notification.
    // The MCP client does not expect a response for notifications (e.g. notifications/initialized).
    id.as_ref()?;

    let response = match req.method.as_str() {
        "initialize" => {
            Response::ok(id, json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": {
                    "name": "engram",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }))
        }

        "initialized" => {
            // Deprecated fallback (should be caught by is_none above if compliant)
            return None;
        }

        "tools/list" => Response::ok(id, tool_list()),

        "tools/call" => {
            let name = params["name"].as_str().unwrap_or("").to_string();
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            let result = handle_tool_call(&name, &args, store);
            Response::ok(id, result)
        }

        "ping" => Response::ok(id, json!({})),

        unknown => {
            warn!("unknown method: {unknown}");
            Response::err(id, -32601, format!("Method not found: {unknown}"))
        }
    };

    Some(response)
}

// ── Server loop ───────────────────────────────────────────────────────────────

/// Run the MCP server, reading from stdin and writing to stdout.
/// Blocks until stdin is closed (i.e. the client disconnects).
pub fn run(store: SharedStore) -> anyhow::Result<()> {
    // ── Boot the Background Worker ─────────────────────────────────
    crate::store::StoreHandle::boot_daemon(store.clone());

    info!("Engram MCP server ready (protocol 2024-11-05)");
    info!("Store: {}", store.lock().unwrap().store_path());

    let stdin  = io::stdin();
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => { error!("stdin read error: {e}"); break; }
        };
        if line.trim().is_empty() { continue; }

        debug!("→ {line}");

        let response_opt = match serde_json::from_str::<Request>(&line) {
            Ok(req) => dispatch(req, &store),
            Err(e) => Some(Response::err(None, -32700, format!("Parse error: {e}"))),
        };

        if let Some(response) = response_opt {
            let out_line = serde_json::to_string(&response)?;
            debug!("← {out_line}");
            writeln!(out, "{out_line}")?;
            out.flush()?;
        }
    }

    info!("Engram MCP server shutdown");
    Ok(())
}
