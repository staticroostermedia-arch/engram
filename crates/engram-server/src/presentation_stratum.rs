//! Presentation stratum — praxis distillation layer for agent rehydration + LEG mirror.
//!
//! Cold manifold (187k+ blocks) stays on NVMe. Wake, harness, and consciousness-surface
//! materialize only a budget-ranked stratum of process/ritual/trace/tile distillates with
//! explicit lineage (summarizes_chain, prev_in_trace, serves).
//!
//! RSI Cycle 22: multi-hop / serves scoring uses RoMem edge volatility α (cost ≈ 1+α)
//! so static structure outranks high-churn succession edges in the wake surface.

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

/// RSI Cycle 42: presentation node budget for lean `session_start` wake path.
/// Slim wake only surfaces ~5 previews; building full K=40 is wasted work.
/// Override: `ENGRAM_WAKE_PRESENTATION_K` (clamped 5..=presentation_budget()).
/// Default **12** under lean; never exceeds [`presentation_budget`].
pub fn presentation_budget_wake() -> usize {
    let full = presentation_budget();
    if let Ok(v) = std::env::var("ENGRAM_WAKE_PRESENTATION_K") {
        if let Ok(n) = v.parse::<usize>() {
            return n.clamp(5, full);
        }
    }
    12.min(full)
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

/// Continuous hop budget for α-weighted multi-hop walks in presentation (default 2.5).
/// Override: `ENGRAM_PRESENTATION_HOP_BUDGET`. ≈ two static hops (1.12×2) or one dynamic.
pub fn presentation_hop_budget() -> f32 {
    if let Ok(v) = std::env::var("ENGRAM_PRESENTATION_HOP_BUDGET") {
        if let Ok(n) = v.parse::<f32>() {
            return n.clamp(1.0, 8.0);
        }
    }
    2.5
}

/// Score multiplier for an edge with volatility α: static edges keep more weight.
/// `score_alpha_scale(0.12) ≈ 0.96`; `score_alpha_scale(0.85) ≈ 0.77`.
/// Honors `ENGRAM_ALPHA_SPEED_GATE` master switch (Cycle 25).
pub fn score_alpha_scale(volatility: f32) -> f32 {
    crate::injection_priority::edge_volatility_scale(volatility)
}

/// Multi-hop labeled walk with edge cost `1+α` and continuous budget (Cycle 21/22).
/// Returns (neighbor, path_cost, edge_α) in visit order, prefer_static expansion first.
pub fn expand_labeled_alpha(
    store: &StoreHandle,
    seed: &str,
    label: &str,
    direction: &str,
    budget: f32,
) -> Vec<(String, f32, f32)> {
    use std::cmp::Ordering;
    use std::collections::{BinaryHeap, HashMap, HashSet};

    #[derive(Clone)]
    struct State {
        cost: f32,
        concept: String,
    }
    impl PartialEq for State {
        fn eq(&self, other: &Self) -> bool {
            self.concept == other.concept && (self.cost - other.cost).abs() < 1e-6
        }
    }
    impl Eq for State {}
    impl PartialOrd for State {
        fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
            Some(self.cmp(other))
        }
    }
    impl Ord for State {
        fn cmp(&self, other: &Self) -> Ordering {
            other
                .cost
                .partial_cmp(&self.cost)
                .unwrap_or(Ordering::Equal)
                .then_with(|| self.concept.cmp(&other.concept))
        }
    }

    let mut best: HashMap<String, f32> = HashMap::new();
    best.insert(seed.to_string(), 0.0);
    let mut heap = BinaryHeap::new();
    heap.push(State {
        cost: 0.0,
        concept: seed.to_string(),
    });
    let mut out: Vec<(String, f32, f32)> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    seen.insert(seed.to_string());

    while let Some(State { cost, concept }) = heap.pop() {
        if cost > best.get(&concept).copied().unwrap_or(f32::MAX) + 1e-5 {
            continue;
        }
        // prefer_static: ascending α
        let mut edges = store.search_relations_ranked(&concept, Some(label), direction, true);
        edges.retain(|(_, other, _)| other != &concept);
        for (_lbl, other, vol) in edges {
            let hop = if crate::injection_priority::alpha_speed_gate_enabled() {
                1.0 + vol
            } else {
                1.0
            };
            let next_cost = cost + hop;
            if next_cost > budget + 1e-5 {
                continue;
            }
            if seen.insert(other.clone()) {
                out.push((other.clone(), next_cost, vol));
            }
            let prev = best.get(&other).copied().unwrap_or(f32::MAX);
            if next_cost + 1e-5 < prev {
                best.insert(other.clone(), next_cost);
                heap.push(State {
                    cost: next_cost,
                    concept: other,
                });
            }
        }
    }
    out
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
    gather_surface_ranked_opts(store, budget, intent, use_intent_recall, false)
}

/// RSI Cycle 47: `lean` skips multi-hop α expand, deep recent scans, and hot-set flood.
pub fn gather_surface_ranked_opts(
    store: &mut StoreHandle,
    budget: usize,
    intent: Option<&str>,
    use_intent_recall: bool,
    lean: bool,
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
        // α-ranked serves: static edges keep higher base score (Cycle 22).
        for (_label, other, vol) in store.search_relations_ranked(goal, Some("serves"), "to", true)
        {
            if let Some(b) = store.fetch_block_high_priority(&other) {
                let text = storage::read_provlog(&b);
                let tt = tile_type_from_provlog(&text);
                let bonus = trusted_tile_bonus(tt.as_deref());
                let base = (0.72 + bonus) * score_alpha_scale(vol);
                push_candidate(
                    &mut candidates,
                    &other,
                    base,
                    b.crs_score,
                    store.is_hot(&other),
                    "goal_serves",
                    "served",
                );
            }
        }
        for (_label, other, vol) in
            store.search_relations_ranked(goal, Some("serves"), "from", true)
        {
            if is_surface_eligible(&other) {
                if let Some(b) = store.fetch_block_high_priority(&other) {
                    let base = 0.64 * score_alpha_scale(vol);
                    push_candidate(
                        &mut candidates,
                        &other,
                        base,
                        b.crs_score,
                        store.is_hot(&other),
                        "goal_served_by",
                        "served",
                    );
                }
            }
        }
    }

    // Trace breadcrumb: α-weighted multi-hop along prev_in_trace from recent head.
    // Cycle 47 lean: single head only, no expand_labeled_alpha (O(deg×budget) on large stalks).
    let hop_budget = presentation_hop_budget();
    let recent_trace_cap = if lean { 12 } else { 40 };
    for (concept, _) in store.access_index.recent(recent_trace_cap) {
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
            if !lean {
                for (prev, path_cost, vol) in
                    expand_labeled_alpha(store, &concept, "prev_in_trace", "to", hop_budget)
                {
                    if is_surface_eligible(&prev) {
                        if let Some(b) = store.fetch_block_high_priority(&prev) {
                            // Deeper / higher-α paths lose base score
                            let depth_pen = (path_cost / hop_budget.max(1.0)).clamp(0.0, 1.0);
                            let base = 0.60 * score_alpha_scale(vol) * (1.0 - 0.25 * depth_pen);
                            push_candidate(
                                &mut candidates,
                                &prev,
                                base,
                                b.crs_score,
                                store.is_hot(&prev),
                                "trace_prev_alpha",
                                "warm",
                            );
                        }
                    }
                }
            }
            break;
        }
    }

    let recent_access_cap = if lean { 24 } else { 80 };
    for (concept, _) in store.access_index.recent(recent_access_cap) {
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

    let mut hot_n = 0usize;
    let hot_cap = if lean { 12 } else { usize::MAX };
    for c in store.hot_concepts() {
        if hot_n >= hot_cap {
            break;
        }
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
            hot_n = hot_n.saturating_add(1);
        }
    }

    if !lean {
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
    }

    if use_intent_recall && !lean {
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
    build_presentation_stratum_opts(store, budget, intent, false)
}

/// RSI Cycle 57: ultra-lean presentation from rehydration hub_anchors only.
/// Skips `gather_surface_ranked` entirely (measured ~0.8s of harness_ms residual).
pub fn build_presentation_stratum_from_hubs(
    store: &StoreHandle,
    hubs: &[String],
    budget: usize,
) -> Value {
    let budget = budget.clamp(1, 12);
    let mut nodes: Vec<Value> = Vec::new();
    let empty_lineage = json!({
        "summarizes_chain": [],
        "prev_in_trace": [],
        "next_in_trace": [],
        "served_by_goals": [],
        "member_count": 0,
        "is_distillate": false,
        "lean_wake": true,
        "hub_only": true,
    });
    for concept in hubs.iter().take(budget) {
        let (preview, crs) = match store.fetch_block_high_priority(concept) {
            Some(b) => {
                let text = storage::read_provlog(&b);
                let p: String = text.chars().take(120).collect();
                let preview = if text.chars().count() > 120 {
                    format!("{}…", p)
                } else {
                    p
                };
                (preview, b.crs_score)
            }
            None => (String::new(), 0.0),
        };
        let kind = if concept.starts_with("tile:") {
            "tile"
        } else if concept.starts_with("trace:") {
            "trace"
        } else if concept.starts_with("goal:") || concept == "primary_goal" {
            "goal"
        } else if concept.starts_with("process:") {
            "process"
        } else if concept.starts_with("helper:") {
            "memory"
        } else {
            "memory"
        };
        nodes.push(json!({
            "concept": concept,
            "kind": kind,
            "crs": crs,
            "hot": store.is_hot(concept),
            "score": 1.0,
            "source": "hub_anchor",
            "orbit": "core",
            "preview": preview,
            "lineage": empty_lineage.clone(),
        }));
    }
    json!({
        "version": "presentation_stratum_v1",
        "node_count": nodes.len(),
        "nodes": nodes,
        "edges": [],
        "lean_wake": true,
        "hub_only": true,
        "budget": budget,
    })
}

/// RSI Cycle 47: lean wake presentation — no multi-hop expand, no per-node lineage walks.
pub fn build_presentation_stratum_opts(
    store: &mut StoreHandle,
    budget: usize,
    intent: Option<&str>,
    lean: bool,
) -> Value {
    let ranked = gather_surface_ranked_opts(store, budget, intent, !lean, lean);
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
                let p: String = text.chars().take(if lean { 120 } else { 200 }).collect();
                if text.len() > if lean { 120 } else { 200 } {
                    format!("{}…", p)
                } else {
                    p
                }
            })
            .unwrap_or_default();

        // Cycle 47 lean: empty lineage (avoids 4× search_relations per node).
        let lineage = if lean {
            json!({
                "summarizes_chain": [],
                "prev_in_trace": [],
                "next_in_trace": [],
                "served_by_goals": [],
                "member_count": 0,
                "is_distillate": false,
                "lean_wake": true,
            })
        } else {
            lineage_for(store, &c.concept)
        };
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

        let l2_norm_residual = if lean {
            0.0
        } else {
            store
                .fetch_block_high_priority(&c.concept)
                .or_else(|| store.fetch_block(&c.concept))
                .map(|b| b.l2_norm_residual)
                .unwrap_or(0.0)
        };
        let mut node = json!({
            "concept": c.concept,
            "kind": kind,
            "crs": c.crs,
            "hot": c.hot,
            "score": c.score,
            "source": c.source,
            "orbit": c.orbit,
            "preview": preview,
            "lineage": lineage,
        });
        if l2_norm_residual > 0.0 {
            node["l2_norm_residual"] = json!(l2_norm_residual);
        }
        nodes.push(node);
    }

    let mut edges: Vec<Value> = Vec::new();
    if selected.contains("primary_goal") {
        // Lean: one ranked serves pass is enough for edge list; skip trace edges.
        for c in selected.iter().filter(|id| *id != "primary_goal") {
            if let Some((_, _, vol)) = store
                .search_relations_ranked("primary_goal", Some("serves"), "to", true)
                .into_iter()
                .find(|(_, t, _)| t == c)
            {
                edges.push(json!({
                    "from": "primary_goal",
                    "to": c,
                    "label": "serves",
                    "volatility": vol,
                    "hop_cost": 1.0 + vol,
                }));
            }
        }
    }
    if !lean {
        for id in selected.iter().filter(|c| c.starts_with("trace:")) {
            for (_label, other, vol) in
                store.search_relations_ranked(id, Some("prev_in_trace"), "both", true)
            {
                if selected.contains(&other) {
                    edges.push(json!({
                        "from": id,
                        "to": other,
                        "label": "prev_in_trace",
                        "volatility": vol,
                        "hop_cost": 1.0 + vol,
                    }));
                }
            }
        }
    }
    if !lean {
        for id in selected.iter().filter(|c| c.starts_with("tile:")) {
            for (_label, member, vol) in
                store.search_relations_ranked(id, Some("summarizes_chain"), "to", true)
            {
                if selected.contains(&member) {
                    edges.push(json!({
                        "from": id,
                        "to": member,
                        "label": "summarizes_chain",
                        "volatility": vol,
                        "hop_cost": 1.0 + vol,
                    }));
                }
            }
        }
    }

    let memory_mode = std::env::var("ENGRAM_MEMORY_MODE").unwrap_or_else(|_| "lean".to_string());
    let primary_goal = crate::store::resolve_active_primary_goal(store);

    json!({
        "version": "v1",
        "praxis": "distilled process/ritual continuation — cold manifold excluded from presentation",
        "budget": budget,
        "hop_budget": presentation_hop_budget(),
        "alpha_weighted": crate::injection_priority::alpha_speed_gate_enabled(),
        "alpha_speed_gate_env": "ENGRAM_ALPHA_SPEED_GATE",
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

    /// RSI Cycle 57: hub-only presentation has hub_only flag and bounded nodes.
    #[test]
    fn hub_only_presentation_from_hubs() {
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        std::env::set_var("ENGRAM_FORCE_CPU_BACKEND", "1");
        let dir = std::env::temp_dir().join(format!(
            "hub_only_pres_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::create_dir_all(&dir);
        let mut store = crate::store::StoreHandle::new(&dir.to_string_lossy());
        let _ = store.remember("primary_goal", "PRIMARY GOAL\n\n**goal:** goal:test\n");
        let _ = store.remember("helper:session_handoff_latest", "handoff");
        let hubs = vec![
            "primary_goal".into(),
            "helper:session_handoff_latest".into(),
        ];
        let s = build_presentation_stratum_from_hubs(&store, &hubs, 8);
        assert_eq!(s.get("hub_only").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(s.get("node_count").and_then(|v| v.as_u64()), Some(2));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RSI Cycle 42: wake presentation K default 12 under lean.
    #[test]
    fn test_presentation_budget_wake_defaults() {
        std::env::remove_var("ENGRAM_PRESENTATION_K");
        std::env::remove_var("ENGRAM_WAKE_PRESENTATION_K");
        std::env::set_var("ENGRAM_MEMORY_MODE", "lean");
        assert_eq!(presentation_budget_wake(), 12);
        std::env::set_var("ENGRAM_WAKE_PRESENTATION_K", "8");
        assert_eq!(presentation_budget_wake(), 8);
        std::env::set_var("ENGRAM_WAKE_PRESENTATION_K", "100");
        assert_eq!(
            presentation_budget_wake(),
            presentation_budget(),
            "clamped to full lean budget"
        );
        std::env::remove_var("ENGRAM_WAKE_PRESENTATION_K");
        std::env::set_var("ENGRAM_MEMORY_MODE", "deep");
        assert_eq!(presentation_budget_wake(), 12); // still default 12; deep full is 64
        std::env::set_var("ENGRAM_WAKE_PRESENTATION_K", "100");
        assert_eq!(presentation_budget_wake(), 64); // clamp to deep full
        std::env::remove_var("ENGRAM_WAKE_PRESENTATION_K");
        std::env::set_var("ENGRAM_MEMORY_MODE", "lean");
    }

    #[test]
    fn score_alpha_scale_prefers_static() {
        let static_s = score_alpha_scale(0.12);
        let dynamic_s = score_alpha_scale(0.85);
        assert!(static_s > dynamic_s);
        assert!((static_s - 1.0 / (1.0 + 0.35 * 0.12)).abs() < 1e-5);
        assert!(score_alpha_scale(0.40) < static_s);
        assert!(score_alpha_scale(0.40) > dynamic_s);
    }

    fn test_store_dir(suffix: &str) -> std::path::PathBuf {
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        std::env::set_var("ENGRAM_FORCE_CPU_BACKEND", "1");
        std::env::set_var("ENGRAM_KI_DISABLE", "1");
        let dir = std::env::temp_dir().join(format!(
            "pres_stratum_{}_{}_{}",
            suffix,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).ok();
        dir
    }

    #[test]
    fn expand_labeled_alpha_respects_hop_budget() {
        let dir = test_store_dir("alpha_hop");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        for c in ["trace:head", "trace:mid", "trace:tail", "trace:dyn"] {
            store.remember(c, &format!("body {c}")).unwrap();
        }
        // Canonical: prev --prev_in_trace--> current (walk with direction "to")
        store
            .relate_with_volatility("trace:mid", "trace:head", "prev_in_trace", Some(0.12))
            .unwrap();
        store
            .relate_with_volatility("trace:tail", "trace:mid", "prev_in_trace", Some(0.12))
            .unwrap();
        store
            .relate_with_volatility("trace:dyn", "trace:head", "prev_in_trace", Some(0.90))
            .unwrap();

        let hop1 = expand_labeled_alpha(&store, "trace:head", "prev_in_trace", "to", 1.5);
        let names1: Vec<&str> = hop1.iter().map(|(c, _, _)| c.as_str()).collect();
        assert!(
            names1.contains(&"trace:mid"),
            "static first hop: {:?}",
            names1
        );
        assert!(
            !names1.contains(&"trace:tail"),
            "second hop over 1.5: {:?}",
            names1
        );
        assert!(
            !names1.contains(&"trace:dyn"),
            "dyn hop cost 1.90 > 1.5: {:?}",
            names1
        );

        let hop2 = expand_labeled_alpha(&store, "trace:head", "prev_in_trace", "to", 2.5);
        let names2: Vec<&str> = hop2.iter().map(|(c, _, _)| c.as_str()).collect();
        assert!(names2.contains(&"trace:mid"));
        assert!(
            names2.contains(&"trace:tail"),
            "static two-hop under 2.5: {:?}",
            names2
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
