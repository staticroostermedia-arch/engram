//! Presentation stratum — praxis distillation layer for agent rehydration + LEG mirror.
//!
//! Cold manifold (187k+ blocks) stays on NVMe. Wake, harness, and consciousness-surface
//! materialize only a budget-ranked stratum of process/ritual/trace/tile distillates with
//! explicit lineage (summarizes_chain, prev_in_trace, serves).

use crate::harness_injection::SESSION_HANDOFF_LATEST;
use crate::store::StoreHandle;
use engram_core::storage;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

/// Agent-shaped concepts eligible for presentation (never bulk AST spatial noise).
pub fn is_surface_eligible(c: &str) -> bool {
    c == "primary_goal"
        || c.starts_with("goal:")
        || c.starts_with("trace:")
        || c.starts_with("tile:")
        || c.starts_with("helper:")
        || c.starts_with("handoff:")
        || c.starts_with("session_end_")
        || c.starts_with("session_start_")
        || c.starts_with("compression_intent_")
        || c.starts_with("compression_handoff_")
        || c.starts_with("manifest:")
        || c.starts_with("uncertainty:")
        || c.starts_with("receipt:session_")
        || c.starts_with("process:engram.")
        || c.starts_with("ritual:")
        || c.starts_with("praxis:")
        || c.starts_with("scar:")
        || c.starts_with("local:")
        || c.starts_with("host:")
        || c.starts_with("env:")
        || c.starts_with("tensor:")
        || c.starts_with("design:")
}

/// Dynamic K from memory mode / env override.
pub fn presentation_budget() -> usize {
    if let Ok(v) = std::env::var("ENGRAM_PRESENTATION_K") {
        if let Ok(n) = v.parse::<usize>() {
            return n.clamp(8, 128);
        }
    }
    match std::env::var("ENGRAM_MEMORY_MODE")
        .unwrap_or_else(|_| "lean".to_string())
        .to_ascii_lowercase()
        .as_str()
    {
        "deep" => 64,
        _ => 40,
    }
}

#[derive(Clone)]
pub(crate) struct Candidate {
    pub(crate) concept: String,
    score: f32,
    crs: f32,
    hot: bool,
    source: String,
    orbit: &'static str,
}

fn tile_type_from_provlog(text: &str) -> Option<String> {
    text.lines()
        .find(|l| l.starts_with("**tile_type:**"))
        .map(|l| l.replace("**tile_type:**", "").trim().to_string())
}

fn recency_boost(store: &StoreHandle, concept: &str, now: u64) -> f32 {
    let ts = store.access_index.last_accessed(concept).unwrap_or(0);
    if ts == 0 {
        return 0.0;
    }
    let age = now.saturating_sub(ts);
    if age < 3600 {
        0.12
    } else if age < 86_400 {
        0.06
    } else if age < 604_800 {
        0.02
    } else {
        0.0
    }
}

fn trusted_tile_bonus(tile_type: Option<&str>) -> f32 {
    match tile_type {
        Some("verified_sequence") => 0.28,
        Some("state_machine") => 0.24,
        Some("formal_spec") => 0.20,
        Some("chain_summary") => 0.18,
        Some("progress") | Some("knowledge_graph") => 0.14,
        _ => 0.0,
    }
}

fn push_candidate(
    candidates: &mut HashMap<String, Candidate>,
    concept: &str,
    base_score: f32,
    crs: f32,
    hot: bool,
    source: &str,
    orbit: &'static str,
) {
    if concept.is_empty() || !is_surface_eligible(concept) {
        return;
    }
    if StoreHandle::is_condensation_tile(concept) {
        return;
    }
    if concept.contains("__fn__") && !concept.ends_with("__arc") {
        return;
    }
    let entry = candidates
        .entry(concept.to_string())
        .or_insert_with(|| Candidate {
            concept: concept.to_string(),
            score: 0.0,
            crs,
            hot,
            source: source.to_string(),
            orbit,
        });
    let new_score = base_score + crs * 0.35 + if hot { 0.15 } else { 0.0 };
    if new_score > entry.score {
        entry.score = new_score;
        entry.crs = crs;
        entry.hot = hot || entry.hot;
        entry.source = source.to_string();
        entry.orbit = orbit;
    } else {
        entry.score = entry.score.max(new_score);
    }
}

fn lineage_for(store: &StoreHandle, concept: &str) -> Value {
    let mut summarizes: Vec<String> = store
        .search_relations(concept, Some("summarizes_chain"), "to")
        .into_iter()
        .map(|(_, c)| c)
        .collect();
    summarizes.sort();

    let prevs: Vec<String> = store
        .search_relations(concept, Some("prev_in_trace"), "to")
        .into_iter()
        .map(|(_, c)| c)
        .collect();
    let nexts: Vec<String> = store
        .search_relations(concept, Some("next_in_trace"), "to")
        .into_iter()
        .map(|(_, c)| c)
        .collect();

    let serves_from: Vec<String> = store
        .search_relations(concept, Some("serves"), "from")
        .into_iter()
        .map(|(_, c)| c)
        .collect();

    json!({
        "summarizes_chain": summarizes,
        "prev_in_trace": prevs,
        "next_in_trace": nexts,
        "served_by_goals": serves_from,
        "member_count": summarizes.len(),
        "is_distillate": !summarizes.is_empty() || concept.starts_with("tile:chain_summary_"),
    })
}

/// Graph-walk candidate gather — primary_goal, handoff, serves, hot/recent/process.
/// `use_intent_recall`: when false, skips nested `recall_scoped` (used by relation-first recall).
pub fn gather_surface_ranked(
    store: &mut StoreHandle,
    budget: usize,
    intent: Option<&str>,
    use_intent_recall: bool,
) -> Vec<Candidate> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut candidates: HashMap<String, Candidate> = HashMap::new();
    let mut primary_goal: Option<String> = None;

    if let Some(block) = store.fetch_block_high_priority("primary_goal") {
        let text = storage::read_provlog(&block);
        if let Some(line) = text.lines().find(|l| l.starts_with("**goal:**")) {
            primary_goal = Some(line.replace("**goal:**", "").trim().to_string());
        }
        push_candidate(
            &mut candidates,
            "primary_goal",
            1.0,
            block.crs_score,
            store.is_hot("primary_goal"),
            "core",
            "core",
        );
    }

    if store
        .fetch_block_high_priority(SESSION_HANDOFF_LATEST)
        .is_some()
    {
        if let Some(b) = store.fetch_block_high_priority(SESSION_HANDOFF_LATEST) {
            push_candidate(
                &mut candidates,
                SESSION_HANDOFF_LATEST,
                0.95,
                b.crs_score,
                store.is_hot(SESSION_HANDOFF_LATEST),
                "handoff",
                "core",
            );
        }
    }

    if let Some(ref goal) = primary_goal {
        for (_label, other) in store.search_relations(goal, Some("serves"), "to") {
            if let Some(b) = store.fetch_block_high_priority(&other) {
                let text = storage::read_provlog(&b);
                let tt = tile_type_from_provlog(&text);
                let bonus = trusted_tile_bonus(tt.as_deref());
                push_candidate(
                    &mut candidates,
                    &other,
                    0.72 + bonus,
                    b.crs_score,
                    store.is_hot(&other),
                    "goal_serves",
                    "served",
                );
            }
        }
        for (_label, other) in store.search_relations(goal, Some("serves"), "from") {
            if is_surface_eligible(&other) {
                if let Some(b) = store.fetch_block_high_priority(&other) {
                    push_candidate(
                        &mut candidates,
                        &other,
                        0.64,
                        b.crs_score,
                        store.is_hot(&other),
                        "goal_served_by",
                        "served",
                    );
                }
            }
        }
    }

    // Trace breadcrumb: one hop along prev_in_trace from the most recent trace head.
    for (concept, _) in store.access_index.recent(40) {
        if concept.starts_with("trace:") {
            push_candidate(
                &mut candidates,
                &concept,
                0.68,
                store
                    .fetch_block_high_priority(&concept)
                    .map(|b| b.crs_score)
                    .unwrap_or(0.74),
                store.is_hot(&concept),
                "trace_head",
                "warm",
            );
            for (_label, prev) in store.search_relations(&concept, Some("prev_in_trace"), "to") {
                if is_surface_eligible(&prev) {
                    if let Some(b) = store.fetch_block_high_priority(&prev) {
                        push_candidate(
                            &mut candidates,
                            &prev,
                            0.60,
                            b.crs_score,
                            store.is_hot(&prev),
                            "trace_prev",
                            "warm",
                        );
                    }
                }
            }
            break;
        }
    }

    for (concept, _) in store.access_index.recent(80) {
        if !is_surface_eligible(&concept) {
            continue;
        }
        if let Some(b) = store.fetch_block_high_priority(&concept) {
            let text = storage::read_provlog(&b);
            let tt = tile_type_from_provlog(&text);
            let bonus = trusted_tile_bonus(tt.as_deref());
            let base = if concept.starts_with("trace:") {
                0.55
            } else if concept.starts_with("process:") {
                0.68
            } else if concept.starts_with("ritual:") {
                0.62
            } else {
                0.50
            };
            push_candidate(
                &mut candidates,
                &concept,
                base + bonus + recency_boost(store, &concept, now),
                b.crs_score,
                store.is_hot(&concept),
                "recent_access",
                "warm",
            );
        }
    }

    for c in store.hot_concepts() {
        if let Some(b) = store.fetch_block_high_priority(&c) {
            push_candidate(
                &mut candidates,
                &c,
                0.58,
                b.crs_score,
                true,
                "hot_set",
                "warm",
            );
        }
    }

    for (concept, _) in store.search_relations("primary_goal", Some("serves"), "from") {
        if concept.starts_with("process:engram.") {
            if let Some(b) = store.fetch_block_high_priority(&concept) {
                push_candidate(
                    &mut candidates,
                    &concept,
                    0.70,
                    b.crs_score,
                    store.is_hot(&concept),
                    "process_sheaf",
                    "served",
                );
            }
        }
    }

    if use_intent_recall {
        if let Some(intent_text) = intent.filter(|s| !s.is_empty()) {
            for mem in store
                .recall_scoped(intent_text, 6, Some("anchors"))
                .0
                .iter()
            {
                if is_surface_eligible(&mem.concept) {
                    push_candidate(
                        &mut candidates,
                        &mem.concept,
                        0.52,
                        mem.crs,
                        store.is_hot(&mem.concept),
                        "intent_recall",
                        "warm",
                    );
                }
            }
        }
    }

    let mut ranked: Vec<Candidate> = candidates.into_values().collect();
    ranked.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                b.crs
                    .partial_cmp(&a.crs)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    ranked.truncate(budget);
    ranked
}

/// Concept names from the relation-first navigation pool (same graph walk as presentation stratum).
pub fn navigable_concept_names(store: &mut StoreHandle, budget: usize) -> Vec<String> {
    gather_surface_ranked(store, budget, None, false)
        .into_iter()
        .map(|c| c.concept)
        .collect()
}

/// Rank and select the presentation stratum — shared by wake bundle, harness, LEG surface.
pub fn build_presentation_stratum(
    store: &mut StoreHandle,
    budget: usize,
    intent: Option<&str>,
) -> Value {
    let ranked = gather_surface_ranked(store, budget, intent, true);
    let selected: HashSet<String> = ranked.iter().map(|c| c.concept.clone()).collect();

    let mut nodes: Vec<Value> = Vec::new();
    let mut lineage_index: HashMap<String, Value> = HashMap::new();
    let mut source_counts: HashMap<String, u32> = HashMap::new();

    for c in &ranked {
        *source_counts.entry(c.source.clone()).or_insert(0) += 1;
        let preview = store
            .fetch_block_high_priority(&c.concept)
            .map(|b| {
                let text = storage::read_provlog(&b);
                let p: String = text.chars().take(200).collect();
                if text.len() > 200 {
                    format!("{}…", p)
                } else {
                    p
                }
            })
            .unwrap_or_default();

        let lineage = lineage_for(store, &c.concept);
        lineage_index.insert(c.concept.clone(), lineage.clone());

        let kind = if c.concept.starts_with("tile:") {
            "tile"
        } else if c.concept.starts_with("trace:") {
            "trace"
        } else if c.concept.starts_with("goal:") || c.concept == "primary_goal" {
            "goal"
        } else if c.concept.starts_with("process:") {
            "process"
        } else if c.concept.starts_with("ritual:") || c.concept.starts_with("praxis:") {
            "ritual"
        } else {
            "memory"
        };

        nodes.push(json!({
            "concept": c.concept,
            "kind": kind,
            "crs": c.crs,
            "hot": c.hot,
            "score": c.score,
            "source": c.source,
            "orbit": c.orbit,
            "preview": preview,
            "lineage": lineage,
        }));
    }

    let mut edges: Vec<Value> = Vec::new();
    if selected.contains("primary_goal") {
        for c in selected.iter().filter(|id| *id != "primary_goal") {
            if store
                .search_relations("primary_goal", Some("serves"), "to")
                .iter()
                .any(|(_, t)| t == c)
            {
                edges.push(json!({
                    "from": "primary_goal",
                    "to": c,
                    "label": "serves",
                }));
            }
        }
    }
    for id in selected.iter().filter(|c| c.starts_with("trace:")) {
        for (_label, other) in store.search_relations(id, Some("prev_in_trace"), "both") {
            if selected.contains(&other) {
                edges.push(json!({
                    "from": id,
                    "to": other,
                    "label": "prev_in_trace",
                }));
            }
        }
    }
    for id in selected.iter().filter(|c| c.starts_with("tile:")) {
        for (_label, member) in store.search_relations(id, Some("summarizes_chain"), "to") {
            if selected.contains(&member) {
                edges.push(json!({
                    "from": id,
                    "to": member,
                    "label": "summarizes_chain",
                }));
            }
        }
    }

    let memory_mode = std::env::var("ENGRAM_MEMORY_MODE").unwrap_or_else(|_| "lean".to_string());
    let primary_goal = crate::store::resolve_active_primary_goal(store);

    json!({
        "version": "v1",
        "praxis": "distilled process/ritual continuation — cold manifold excluded from presentation",
        "budget": budget,
        "memory_mode": memory_mode,
        "node_count": nodes.len(),
        "primary_goal": primary_goal,
        "nodes": nodes,
        "edges": edges,
        "lineage_index": lineage_index,
        "distillate_sources": source_counts,
        "cold_excluded": {
            "spatial_ast_bulk": true,
            "full_manifold_list": true,
            "note": "Dig via recall(scope=all) or context_for_edit at locus only",
        },
        "training_note": "Ritualized .leg blocks + lineage edges are richer than flat JSON reasoning traces for long-horizon agent training export",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_surface_eligible_excludes_ast_fn() {
        assert!(is_surface_eligible("trace:foo"));
        assert!(is_surface_eligible("process:engram.meta.agent-evolution"));
        assert!(!is_surface_eligible("store__fn__context_for_edit"));
    }

    #[test]
    fn test_presentation_budget_defaults() {
        std::env::remove_var("ENGRAM_PRESENTATION_K");
        std::env::set_var("ENGRAM_MEMORY_MODE", "lean");
        assert_eq!(presentation_budget(), 40);
        std::env::set_var("ENGRAM_MEMORY_MODE", "deep");
        assert_eq!(presentation_budget(), 64);
    }
}
