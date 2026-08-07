//! MCP dispatch helpers for cognitive OS extensions (E1–E9).

use crate::store::StoreHandle;
use serde_json::{json, Value};
use std::path::Path;
use std::sync::{Arc, Mutex};

type Store = Arc<Mutex<StoreHandle>>;

fn lock_store(store: &Store) -> Result<std::sync::MutexGuard<'_, StoreHandle>, Value> {
    store.lock().map_err(|p| {
        json!({
            "content": [{"type": "text", "text": format!("Error: store mutex poisoned: {p}")}],
            "isError": true
        })
    })
}

fn ok_json(v: Value) -> Value {
    json!({
        "content": [{"type": "text", "text": serde_json::to_string_pretty(&v).unwrap_or_else(|_| v.to_string())}]
    })
}

fn err_text(msg: &str) -> Value {
    json!({
        "content": [{"type": "text", "text": format!("Error: {msg}")}],
        "isError": true
    })
}

pub fn tool_schemas() -> Vec<Value> {
    vec![
        json!({
            "name": "mcp_engram_expand_wake",
            "description": "E1: Expand one wake slot (edit_arc|scars|tiles|trust_residual|presentation|full_continuation|any continuation key) without full re-wake. Optional max_tokens truncates.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "slot": {"type": "string", "description": "Slot name to expand"},
                    "max_tokens": {"type": "integer", "description": "Optional token budget for slot payload"}
                },
                "required": ["slot"]
            }
        }),
        json!({
            "name": "mcp_engram_query_structured",
            "description": "E8: Structured filter query — type_prefix, crs_min/max, related_to, path_contains, limit, order_by. Additive power tool; lean default remains recall(scope=anchors).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "type_prefix": {"type": "string"},
                    "crs_min": {"type": "number"},
                    "crs_max": {"type": "number"},
                    "related_to": {"type": "string"},
                    "direction": {"type": "string", "description": "in|out|both"},
                    "relation_label": {"type": "string"},
                    "file_stem": {"type": "string"},
                    "path_contains": {"type": "string"},
                    "limit": {"type": "integer"},
                    "order_by": {"type": "string", "description": "crs|recency"},
                    "include_foreign": {"type": "boolean"}
                }
            }
        }),
        json!({
            "name": "mcp_engram_topology_health",
            "description": "E7: Sample-capped manifold topology health (orphan_rate, hub_dominance, scar_density, suggestions). Never full O(N²).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "sample_limit": {"type": "integer"}
                }
            }
        }),
        json!({
            "name": "mcp_engram_branch_create",
            "description": "E3: Create counterfactual branch from trace/goal.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "from_trace": {"type": "string"},
                    "from_goal": {"type": "string"},
                    "label": {"type": "string"}
                },
                "required": ["label"]
            }
        }),
        json!({
            "name": "mcp_engram_branch_checkout",
            "description": "E3: Checkout branch id or 'main' for session-scoped writes/wake.",
            "inputSchema": {
                "type": "object",
                "properties": {"branch_id": {"type": "string"}},
                "required": ["branch_id"]
            }
        }),
        json!({
            "name": "mcp_engram_branch_merge",
            "description": "E3: Merge branch with receipt.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "branch_id": {"type": "string"},
                    "strategy": {"type": "string", "description": "manual|prefer_branch|prefer_main"}
                },
                "required": ["branch_id"]
            }
        }),
        json!({
            "name": "mcp_engram_branch_abandon",
            "description": "E3: Abandon branch with scar.",
            "inputSchema": {
                "type": "object",
                "properties": {"branch_id": {"type": "string"}},
                "required": ["branch_id"]
            }
        }),
        json!({
            "name": "mcp_engram_lease_acquire",
            "description": "E5: Acquire single-concept lease (TTL ms).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "concept": {"type": "string"},
                    "agent_id": {"type": "string"},
                    "ttl_ms": {"type": "integer"}
                },
                "required": ["concept", "agent_id"]
            }
        }),
        json!({
            "name": "mcp_engram_lease_release",
            "description": "E5: Release lease held by agent_id.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "concept": {"type": "string"},
                    "agent_id": {"type": "string"}
                },
                "required": ["concept", "agent_id"]
            }
        }),
        json!({
            "name": "mcp_engram_lease_break",
            "description": "E5: Admin break-glass release of a stuck lease.",
            "inputSchema": {
                "type": "object",
                "properties": {"concept": {"type": "string"}},
                "required": ["concept"]
            }
        }),
        json!({
            "name": "mcp_engram_ingest_external",
            "description": "E9: Ingest local file/text as foreign low-CRS external knowledge (excluded from anchors by default).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "text": {"type": "string"},
                    "source_label": {"type": "string"},
                    "mime": {"type": "string"}
                },
                "required": ["source_label"]
            }
        }),
        json!({
            "name": "mcp_engram_accept_external",
            "description": "E9: Accept foreign concept for pin/promote eligibility after verify.",
            "inputSchema": {
                "type": "object",
                "properties": {"concept": {"type": "string"}},
                "required": ["concept"]
            }
        }),
        json!({
            "name": "mcp_engram_sync_export",
            "description": "E6: Export goal-scoped continuity pack (engram_sync_pack_v1) to out_path directory.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "goal_id": {"type": "string"},
                    "min_crs": {"type": "number"},
                    "out_path": {"type": "string"}
                },
                "required": ["goal_id", "out_path"]
            }
        }),
        json!({
            "name": "mcp_engram_sync_import",
            "description": "E6: Import sync pack with quarantine CRS (default true).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "quarantine": {"type": "boolean"}
                },
                "required": ["path"]
            }
        }),
        json!({
            "name": "mcp_engram_distill_skills",
            "description": "E2: Cluster repeated successful traces into skill_draft:* (capped; no auto-pin).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "min_repeats": {"type": "integer"},
                    "max_drafts": {"type": "integer"},
                    "goal_filter": {"type": "string"}
                }
            }
        }),
        json!({
            "name": "mcp_engram_promote_skill_draft",
            "description": "E2: Promote skill draft after harness checklist (or scar on fail).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": {"type": "string"},
                    "harness_pass": {"type": "boolean"}
                },
                "required": ["id"]
            }
        }),
        json!({
            "name": "mcp_engram_dream_run",
            "description": "E4: Offline dream curriculum — probe anchors, score hit@k, write metric:dream_*. Auto-schedule off on minimal.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "k": {"type": "integer"},
                    "max_probes": {"type": "integer"}
                }
            }
        }),
    ]
}

pub fn handle(name: &str, args: &Value, store: &Store) -> Option<Value> {
    Some(match name {
        "mcp_engram_expand_wake" => expand_wake(args, store),
        "mcp_engram_query_structured" => query_structured(args, store),
        "mcp_engram_topology_health" => topology_health(args, store),
        "mcp_engram_branch_create" => {
            let label = args
                .get("label")
                .and_then(|v| v.as_str())
                .unwrap_or("branch");
            let from = args
                .get("from_trace")
                .or_else(|| args.get("from_goal"))
                .and_then(|v| v.as_str())
                .unwrap_or("mainline");
            ok_json(crate::branch_memory::branch_create(from, label))
        }
        "mcp_engram_branch_checkout" => {
            let id = args
                .get("branch_id")
                .and_then(|v| v.as_str())
                .unwrap_or("main");
            ok_json(crate::branch_memory::branch_checkout(id))
        }
        "mcp_engram_branch_merge" => {
            let id = args.get("branch_id").and_then(|v| v.as_str()).unwrap_or("");
            let strat = args
                .get("strategy")
                .and_then(|v| v.as_str())
                .unwrap_or("prefer_branch");
            ok_json(crate::branch_memory::branch_merge(id, strat))
        }
        "mcp_engram_branch_abandon" => {
            let id = args.get("branch_id").and_then(|v| v.as_str()).unwrap_or("");
            ok_json(crate::branch_memory::branch_abandon(id))
        }
        "mcp_engram_lease_acquire" => {
            let c = args.get("concept").and_then(|v| v.as_str()).unwrap_or("");
            let a = args.get("agent_id").and_then(|v| v.as_str()).unwrap_or("");
            let ttl = args
                .get("ttl_ms")
                .and_then(|v| v.as_u64())
                .unwrap_or(30_000);
            if c.is_empty() || a.is_empty() {
                return Some(err_text("concept and agent_id required"));
            }
            ok_json(crate::lease_conflict::lease_acquire(c, a, ttl))
        }
        "mcp_engram_lease_release" => {
            let c = args.get("concept").and_then(|v| v.as_str()).unwrap_or("");
            let a = args.get("agent_id").and_then(|v| v.as_str()).unwrap_or("");
            ok_json(crate::lease_conflict::lease_release(c, a))
        }
        "mcp_engram_lease_break" => {
            let c = args.get("concept").and_then(|v| v.as_str()).unwrap_or("");
            ok_json(crate::lease_conflict::lease_break(c))
        }
        "mcp_engram_ingest_external" => ingest_external(args, store),
        "mcp_engram_accept_external" => {
            let c = args.get("concept").and_then(|v| v.as_str()).unwrap_or("");
            ok_json(crate::foreign_stratum::accept_external(c))
        }
        "mcp_engram_sync_export" => sync_export(args, store),
        "mcp_engram_sync_import" => sync_import(args, store),
        "mcp_engram_distill_skills" => distill_skills(args, store),
        "mcp_engram_promote_skill_draft" => {
            let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let pass = args
                .get("harness_pass")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            ok_json(crate::skill_distill::promote_draft(id, pass))
        }
        "mcp_engram_dream_run" => dream_run(args, store),
        _ => return None,
    })
}

fn expand_wake(args: &Value, store: &Store) -> Value {
    let slot = match args.get("slot").and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s,
        _ => return err_text("slot required"),
    };
    let max_tokens = args
        .get("max_tokens")
        .and_then(|v| v.as_u64())
        .map(|n| n as usize);
    let mut lock = match lock_store(store) {
        Ok(l) => l,
        Err(e) => return e,
    };
    let full = lock.build_continuation_bundle_wake(None);
    let expanded = crate::wake_budget::expand_wake_slot(&full, slot, max_tokens);
    ok_json(expanded)
}

fn query_structured(args: &Value, store: &Store) -> Value {
    let q = crate::structured_query::StructuredQuery::from_json(args);
    let include_foreign = args
        .get("include_foreign")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let mut lock = match lock_store(store) {
        Ok(l) => l,
        Err(e) => return e,
    };
    // Cap list scan
    let names = lock.list();
    let cap = 5_000.min(names.len());
    let mut rows = Vec::with_capacity(cap.min(512));
    for concept in names.into_iter().take(cap) {
        if !include_foreign && crate::foreign_stratum::is_foreign(&concept) {
            continue;
        }
        let crs = lock
            .fetch_block(&concept)
            .map(|b| b.crs_score)
            .unwrap_or(0.5);
        let recency = lock.access_index.last_accessed(&concept).unwrap_or(0);
        let mut edges_out = Vec::new();
        let mut edges_in = Vec::new();
        // Use relation index if available via search helpers
        if let Some(ref seed) = q.related_to {
            // cheap: only fill edges when related_to filter present
            let _ = seed;
        }
        // Pull neighbors for this concept (bounded)
        let rels = lock.search_relations_ranked(&concept, None, "both", true);
        for (label, neigh, _vol) in rels.into_iter().take(16) {
            edges_out.push((neigh, label));
        }
        rows.push(crate::structured_query::QueryRow {
            concept,
            crs,
            recency,
            path: None,
            edges_out,
            edges_in,
            foreign: false,
        });
        if rows.len() >= 2_000 {
            break;
        }
    }
    let filtered = crate::structured_query::filter_rows(&rows, &q);
    ok_json(crate::structured_query::rows_to_json(&filtered))
}

fn topology_health(args: &Value, store: &Store) -> Value {
    let host = std::env::var("ENGRAM_HOST_PROFILE_ACTIVE")
        .or_else(|_| std::env::var("ENGRAM_HOST_PROFILE"))
        .unwrap_or_else(|_| "auto".into());
    let default_lim = crate::topology_health::default_sample_limit(&host);
    let sample_limit = args
        .get("sample_limit")
        .and_then(|v| v.as_u64())
        .map(|n| n as usize)
        .unwrap_or(default_lim);
    let lock = match lock_store(store) {
        Ok(l) => l,
        Err(e) => return e,
    };
    let names = lock.list();
    let nodes: Vec<crate::topology_health::TopoNode> = names
        .iter()
        .take(sample_limit.saturating_mul(2).max(sample_limit))
        .map(|c| crate::topology_health::TopoNode {
            concept: c.clone(),
            is_scar: c.starts_with("scar:"),
        })
        .collect();
    // Sample edges via relation index if present
    let mut edges = Vec::new();
    for c in names.iter().take(sample_limit.min(500)) {
        let rels = lock.search_relations_ranked(c, None, "from", true);
        for (label, to, _vol) in rels.into_iter().take(8) {
            if !to.is_empty() {
                edges.push(crate::topology_health::TopoEdge {
                    from: c.clone(),
                    to,
                    label,
                });
            }
        }
    }
    let mut report = crate::topology_health::compute_topology_health(&nodes, &edges, sample_limit);
    // Optional persist metric
    if let Some(obj) = report.as_object_mut() {
        obj.insert("host_profile_hint".into(), json!(host));
    }
    ok_json(report)
}

fn ingest_external(args: &Value, store: &Store) -> Value {
    let source = args
        .get("source_label")
        .and_then(|v| v.as_str())
        .unwrap_or("external");
    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
    if !path.is_empty() && !crate::foreign_stratum::path_allowed(path) {
        return err_text("URL fetch disabled (set ENGRAM_EXTERNAL_URL_FETCH=1 to allow)");
    }
    let text = if let Some(t) = args.get("text").and_then(|v| v.as_str()) {
        t.to_string()
    } else if !path.is_empty() {
        match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => return err_text(&format!("read path failed: {e}")),
        }
    } else {
        return err_text("path or text required");
    };
    let path_label = if path.is_empty() { "inline" } else { path };
    let (concept, body, crs) =
        crate::foreign_stratum::build_foreign_payload(source, &text, path_label);
    let mut lock = match lock_store(store) {
        Ok(l) => l,
        Err(e) => return e,
    };
    let mut block = lock.encode(&body);
    block.crs_score = crs;
    if let Err(e) = lock.store(&concept, block) {
        return err_text(&format!("store failed: {e}"));
    }
    crate::foreign_stratum::register_foreign(&concept);
    crate::branch_memory::tag_write(&concept);
    ok_json(json!({
        "ok": true,
        "concept": concept,
        "foreign": true,
        "crs": crs,
        "version": "foreign_stratum_v1",
    }))
}

fn sync_export(args: &Value, store: &Store) -> Value {
    let goal = args.get("goal_id").and_then(|v| v.as_str()).unwrap_or("");
    let out = args.get("out_path").and_then(|v| v.as_str()).unwrap_or("");
    let min_crs = args.get("min_crs").and_then(|v| v.as_f64()).unwrap_or(0.74) as f32;
    if goal.is_empty() || out.is_empty() {
        return err_text("goal_id and out_path required");
    }
    let lock = match lock_store(store) {
        Ok(l) => l,
        Err(e) => return e,
    };
    let mut items = Vec::new();
    // Include goal + high-CRS neighbors
    let candidates = {
        let mut c = vec![goal.to_string(), "primary_goal".into()];
        for (_lbl, n, _vol) in lock
            .search_relations_ranked(goal, None, "both", true)
            .into_iter()
            .take(32)
        {
            c.push(n);
        }
        c
    };
    for concept in candidates {
        if let Some(b) = lock.fetch_block(&concept) {
            if b.crs_score >= min_crs || concept == goal {
                let text = String::from_utf8_lossy(&b.payload)
                    .trim_matches('\0')
                    .to_string();
                items.push(crate::sync_pack::PackItem {
                    concept: concept.clone(),
                    text,
                    crs: b.crs_score,
                    kind: concept.split(':').next().unwrap_or("block").to_string(),
                });
            }
        }
    }
    let host = std::env::var("HOSTNAME").unwrap_or_else(|_| "local".into());
    let man = crate::sync_pack::build_manifest(&host, &items, goal);
    match crate::sync_pack::write_pack(Path::new(out), &man, &items) {
        Ok(()) => ok_json(json!({
            "ok": true,
            "out_path": out,
            "item_count": items.len(),
            "manifest": man,
        })),
        Err(e) => err_text(&e),
    }
}

fn sync_import(args: &Value, store: &Store) -> Value {
    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
    let quarantine = args
        .get("quarantine")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    if path.is_empty() {
        return err_text("path required");
    }
    let (man, items) = match crate::sync_pack::read_pack(Path::new(path)) {
        Ok(x) => x,
        Err(e) => return err_text(&format!("pack rejected: {e}")),
    };
    let mut lock = match lock_store(store) {
        Ok(l) => l,
        Err(e) => return e,
    };
    let mut imported = 0usize;
    for it in &items {
        let crs = if quarantine {
            crate::sync_pack::quarantine_crs(it.crs)
        } else {
            it.crs
        };
        let body = format!(
            "{}\n\n**source:sync**\n**quarantine:** {quarantine}\n**crs:** {crs}\n",
            it.text
        );
        let mut block = lock.encode(&body);
        block.crs_score = crs;
        let concept = if lock.fetch_block(&it.concept).is_some() {
            format!("sync:{}", it.concept)
        } else {
            it.concept.clone()
        };
        if lock.store(&concept, block).is_ok() {
            crate::foreign_stratum::register_foreign(&concept);
            imported += 1;
        }
    }
    let mut summary = crate::sync_pack::import_summary(&items, quarantine);
    if let Some(obj) = summary.as_object_mut() {
        obj.insert("imported_count".into(), json!(imported));
        obj.insert(
            "manifest_schema".into(),
            man.get("schema").cloned().unwrap_or(json!(null)),
        );
    }
    ok_json(summary)
}

fn distill_skills(args: &Value, store: &Store) -> Value {
    let min_repeats = args
        .get("min_repeats")
        .and_then(|v| v.as_u64())
        .unwrap_or(3) as usize;
    let max_drafts = args
        .get("max_drafts")
        .and_then(|v| v.as_u64())
        .unwrap_or(20) as usize;
    let goal_filter = args
        .get("goal_filter")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let lock = match lock_store(store) {
        Ok(l) => l,
        Err(e) => return e,
    };
    let names = lock.list();
    let mut fps = Vec::new();
    for c in names
        .into_iter()
        .filter(|c| c.starts_with("trace:"))
        .take(2_000)
    {
        if let Some(b) = lock.fetch_block(&c) {
            let text = String::from_utf8_lossy(&b.payload).to_string();
            if !goal_filter.is_empty() && !text.contains(goal_filter) {
                continue;
            }
            // crude parse
            let decision = text
                .lines()
                .find(|l| l.contains("decision") || l.starts_with("**decision"))
                .unwrap_or("unknown decision")
                .to_string();
            let spatial = text
                .lines()
                .find(|l| l.contains("spatial") || l.contains(".rs"))
                .unwrap_or("")
                .to_string();
            fps.push(crate::skill_distill::TraceFingerprint {
                concept: c,
                decision_point: decision,
                spatial_stem: crate::skill_distill::spatial_stem(&spatial),
                tool_sequence: "edit,test".into(),
                crs: b.crs_score,
                success: b.crs_score >= 0.74,
            });
        }
    }
    let out = crate::skill_distill::distill_drafts(&fps, min_repeats, max_drafts);
    // Persist draft concepts when present
    if let Some(drafts) = out.get("drafts").and_then(|d| d.as_array()) {
        drop(lock);
        if let Ok(mut lock) = store.lock() {
            for d in drafts {
                if let Some(id) = d.get("id").and_then(|v| v.as_str()) {
                    let body = format!(
                        "SKILL DRAFT\n\n{}\n",
                        serde_json::to_string_pretty(d).unwrap_or_default()
                    );
                    let mut block = lock.encode(&body);
                    block.crs_score = 0.72;
                    let _ = lock.store(id, block);
                }
            }
        }
    }
    ok_json(out)
}

fn dream_run(args: &Value, store: &Store) -> Value {
    let host = std::env::var("ENGRAM_HOST_PROFILE_ACTIVE")
        .or_else(|_| std::env::var("ENGRAM_HOST_PROFILE"))
        .unwrap_or_else(|_| "auto".into());
    let k = args.get("k").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
    let max_probes = args
        .get("max_probes")
        .and_then(|v| v.as_u64())
        .unwrap_or(16) as usize;
    let mut lock = match lock_store(store) {
        Ok(l) => l,
        Err(e) => return e,
    };
    let names = lock.list();
    let anchors: Vec<String> = names
        .into_iter()
        .filter(|c| {
            c.starts_with("goal:")
                || c.starts_with("tile:")
                || c == "primary_goal"
                || c.starts_with("helper:session_")
        })
        .take(max_probes)
        .collect();
    let mut probes = Vec::new();
    for concept in &anchors {
        let query = concept.replace(':', " ");
        let hits_raw = lock.recall(&query, k);
        let hits: Vec<crate::dream_curriculum::DreamHit> = hits_raw
            .into_iter()
            .map(|m| crate::dream_curriculum::DreamHit {
                concept: m.concept.clone(),
                score: m.crs,
            })
            .collect();
        probes.push((
            crate::dream_curriculum::DreamProbe {
                concept: concept.clone(),
                query,
            },
            hits,
        ));
    }
    let result = crate::dream_curriculum::run_dream(&probes, k);
    let metric = result
        .get("metric_concept")
        .and_then(|v| v.as_str())
        .unwrap_or("metric:dream_latest")
        .to_string();
    let body = crate::dream_curriculum::metric_block_text(&result);
    let mut block = lock.encode(&body);
    block.crs_score = 0.85;
    let _ = lock.store(&metric, block);
    let mut out = result;
    if let Some(obj) = out.as_object_mut() {
        obj.insert(
            "dream_auto_schedule_enabled".into(),
            json!(crate::dream_curriculum::dream_auto_schedule_enabled(&host)),
        );
        obj.insert("host_profile_hint".into(), json!(host));
    }
    ok_json(out)
}
