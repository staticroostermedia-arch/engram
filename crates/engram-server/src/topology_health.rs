//! E7 — Manifold topology health: capped sample metrics + suggestions.

use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct TopoEdge {
    pub from: String,
    pub to: String,
    #[allow(dead_code)]
    pub label: String,
}

#[derive(Debug, Clone)]
pub struct TopoNode {
    pub concept: String,
    pub is_scar: bool,
}

/// Sample-capped topology health. Never O(N²) beyond sample_limit nodes.
pub fn compute_topology_health(
    nodes: &[TopoNode],
    edges: &[TopoEdge],
    sample_limit: usize,
) -> Value {
    let sample_limit = sample_limit.clamp(16, 50_000);
    let n_take = nodes.len().min(sample_limit);
    let sample_nodes: Vec<&TopoNode> = nodes.iter().take(n_take).collect();
    let sample_set: HashSet<&str> = sample_nodes.iter().map(|n| n.concept.as_str()).collect();

    let mut degree: HashMap<&str, usize> = HashMap::new();
    let mut edge_count = 0usize;
    for e in edges {
        if sample_set.contains(e.from.as_str()) || sample_set.contains(e.to.as_str()) {
            edge_count += 1;
            *degree.entry(e.from.as_str()).or_default() += 1;
            *degree.entry(e.to.as_str()).or_default() += 1;
        }
    }

    let mut with_rel = 0usize;
    for n in &sample_nodes {
        if degree.get(n.concept.as_str()).copied().unwrap_or(0) > 0 {
            with_rel += 1;
        }
    }
    let orphan_count = n_take.saturating_sub(with_rel);
    let orphan_rate = if n_take == 0 {
        0.0
    } else {
        orphan_count as f64 / n_take as f64
    };

    let mut max_deg = 0usize;
    let mut hub = "";
    for (c, d) in &degree {
        if *d > max_deg {
            max_deg = *d;
            hub = c;
        }
    }
    let hub_dominance = if edge_count == 0 {
        0.0
    } else {
        max_deg as f64 / edge_count as f64
    };

    let scar_count = sample_nodes.iter().filter(|n| n.is_scar).count();
    let scar_density = if n_take == 0 {
        0.0
    } else {
        scar_count as f64 / n_take as f64
    };

    let mut suggestions: Vec<Value> = Vec::new();
    if orphan_rate > 0.4 {
        suggestions.push(json!({
            "action": "relate_orphans",
            "reason": format!("orphan_rate={orphan_rate:.2} — many unconnected concepts in sample"),
        }));
    }
    if hub_dominance > 0.25 && !hub.is_empty() {
        suggestions.push(json!({
            "action": "demote_or_fanout_hub",
            "hub": hub,
            "reason": format!("hub_dominance={hub_dominance:.2} on {hub}"),
        }));
    }
    if scar_density > 0.15 {
        suggestions.push(json!({
            "action": "review_scars",
            "reason": format!("scar_density={scar_density:.2}"),
        }));
    }
    if suggestions.is_empty() {
        suggestions.push(json!({
            "action": "nominal",
            "reason": "sample topology within soft bands",
        }));
    }

    json!({
        "version": "topology_health_v1",
        "sample_limit": sample_limit,
        "nodes_sampled": n_take,
        "nodes_total_hint": nodes.len(),
        "edges_in_sample": edge_count,
        "orphan_count": orphan_count,
        "orphan_rate": orphan_rate,
        "hub": hub,
        "hub_degree": max_deg,
        "hub_dominance": hub_dominance,
        "scar_count": scar_count,
        "scar_density": scar_density,
        "capped": nodes.len() > sample_limit,
        "suggestions": suggestions,
    })
}

/// Default sample by host profile.
pub fn default_sample_limit(host_profile: &str) -> usize {
    match host_profile {
        "minimal" | "host_minimal" => 256,
        "cuda_low_vram" | "host_cuda_low_vram" => 512,
        "cuda_dual" | "host_cuda_dual" => 4096,
        _ => 1024,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_orphan_and_hub() {
        let nodes: Vec<TopoNode> = (0..10)
            .map(|i| TopoNode {
                concept: format!("n{i}"),
                is_scar: i == 9,
            })
            .collect();
        // Star: n0 hub to n1..n4; n5..n8 orphans; n9 scar orphan
        let edges: Vec<TopoEdge> = (1..5)
            .map(|i| TopoEdge {
                from: "n0".into(),
                to: format!("n{i}"),
                label: "x".into(),
            })
            .collect();
        let h = compute_topology_health(&nodes, &edges, 100);
        assert!(h["orphan_rate"].as_f64().unwrap() > 0.3);
        assert_eq!(h["hub"], "n0");
        assert!(h["hub_dominance"].as_f64().unwrap() > 0.0);
        assert!(h["scar_density"].as_f64().unwrap() > 0.0);
    }

    #[test]
    fn sample_cap_respected() {
        let nodes: Vec<TopoNode> = (0..1000)
            .map(|i| TopoNode {
                concept: format!("c{i}"),
                is_scar: false,
            })
            .collect();
        let h = compute_topology_health(&nodes, &[], 50);
        assert_eq!(h["nodes_sampled"], 50);
        assert_eq!(h["capped"], true);
    }

    #[test]
    fn minimal_sample_lt_dual() {
        assert!(default_sample_limit("minimal") < default_sample_limit("cuda_dual"));
    }
}
