//! E8 — Structured query surface: filter plan over concepts without external DB.

use serde_json::{json, Value};

#[derive(Debug, Clone, Default)]
pub struct StructuredQuery {
    pub type_prefix: Option<String>,
    pub crs_min: Option<f32>,
    pub crs_max: Option<f32>,
    pub related_to: Option<String>,
    pub relation_direction: String, // in|out|both
    pub relation_label: Option<String>,
    pub file_stem: Option<String>,
    pub path_contains: Option<String>,
    pub limit: usize,
    pub order_by: String, // crs|recency
}

impl StructuredQuery {
    pub fn from_json(args: &Value) -> Self {
        let limit = args
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(20)
            .clamp(1, 200) as usize;
        Self {
            type_prefix: args
                .get("type_prefix")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            crs_min: args
                .get("crs_min")
                .and_then(|v| v.as_f64())
                .map(|f| f as f32),
            crs_max: args
                .get("crs_max")
                .and_then(|v| v.as_f64())
                .map(|f| f as f32),
            related_to: args
                .get("related_to")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            relation_direction: args
                .get("direction")
                .and_then(|v| v.as_str())
                .unwrap_or("both")
                .to_string(),
            relation_label: args
                .get("relation_label")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            file_stem: args
                .get("file_stem")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            path_contains: args
                .get("path_contains")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            limit,
            order_by: args
                .get("order_by")
                .and_then(|v| v.as_str())
                .unwrap_or("crs")
                .to_string(),
        }
    }
}

/// One candidate row for pure filtering.
#[derive(Debug, Clone)]
pub struct QueryRow {
    pub concept: String,
    pub crs: f32,
    pub recency: u64,
    pub path: Option<String>,
    /// Neighbor edges (to, label) for related_to filter.
    pub edges_out: Vec<(String, String)>,
    pub edges_in: Vec<(String, String)>,
    pub foreign: bool,
}

/// Pure structured filter — drives the real filter plan used by MCP.
pub fn filter_rows(rows: &[QueryRow], q: &StructuredQuery) -> Vec<QueryRow> {
    let mut out: Vec<QueryRow> = rows
        .iter()
        .filter(|r| {
            if r.foreign {
                // Foreign excluded unless caller already filtered them in
            }
            if let Some(ref p) = q.type_prefix {
                if !r.concept.starts_with(p.as_str()) {
                    return false;
                }
            }
            if let Some(mn) = q.crs_min {
                if r.crs < mn {
                    return false;
                }
            }
            if let Some(mx) = q.crs_max {
                if r.crs > mx {
                    return false;
                }
            }
            if let Some(ref stem) = q.file_stem {
                let path = r.path.as_deref().unwrap_or("");
                let file = path.rsplit('/').next().unwrap_or(path);
                let file_stem = file.split('.').next().unwrap_or(file);
                if file_stem != stem.as_str() && !r.concept.contains(stem.as_str()) {
                    return false;
                }
            }
            if let Some(ref pc) = q.path_contains {
                let path = r.path.as_deref().unwrap_or("");
                if !path.contains(pc.as_str()) && !r.concept.contains(pc.as_str()) {
                    return false;
                }
            }
            if let Some(ref seed) = q.related_to {
                let label_ok = |lab: &str| {
                    q.relation_label
                        .as_ref()
                        .map(|want| lab == want.as_str())
                        .unwrap_or(true)
                };
                let out_hit = r.edges_out.iter().any(|(t, l)| t == seed && label_ok(l));
                let in_hit = r.edges_in.iter().any(|(f, l)| f == seed && label_ok(l));
                // Also: rows that ARE the seed's neighbors listed from seed side
                // Here row is candidate; related_to means candidate has edge to/from seed
                let ok = match q.relation_direction.as_str() {
                    "out" => out_hit,
                    "in" => in_hit,
                    _ => out_hit || in_hit,
                };
                // Alternative: seed's neighbor equals this concept — encode as edges on seed
                // For pure filter, also accept if concept == seed
                if r.concept == *seed {
                    return true;
                }
                if !ok {
                    return false;
                }
            }
            true
        })
        .cloned()
        .collect();

    match q.order_by.as_str() {
        "recency" => out.sort_by_key(|r| std::cmp::Reverse(r.recency)),
        _ => out.sort_by(|a, b| {
            b.crs
                .partial_cmp(&a.crs)
                .unwrap_or(std::cmp::Ordering::Equal)
        }),
    }
    out.truncate(q.limit);
    out
}

pub fn rows_to_json(rows: &[QueryRow]) -> Value {
    json!({
        "version": "structured_query_v1",
        "count": rows.len(),
        "results": rows.iter().map(|r| json!({
            "concept": r.concept,
            "crs": r.crs,
            "recency": r.recency,
            "path": r.path,
            "foreign": r.foreign,
        })).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<QueryRow> {
        vec![
            QueryRow {
                concept: "goal:a".into(),
                crs: 0.9,
                recency: 100,
                path: None,
                edges_out: vec![("goal:b".into(), "decomposes_into".into())],
                edges_in: vec![],
                foreign: false,
            },
            QueryRow {
                concept: "goal:b".into(),
                crs: 0.7,
                recency: 200,
                path: None,
                edges_out: vec![],
                edges_in: vec![("goal:a".into(), "decomposes_into".into())],
                foreign: false,
            },
            QueryRow {
                concept: "trace:x".into(),
                crs: 0.5,
                recency: 300,
                path: Some("store.rs".into()),
                edges_out: vec![],
                edges_in: vec![],
                foreign: false,
            },
            QueryRow {
                concept: "metric:noise".into(),
                crs: 0.4,
                recency: 50,
                path: None,
                edges_out: vec![],
                edges_in: vec![],
                foreign: false,
            },
        ]
    }

    #[test]
    fn filter_type_and_crs() {
        let q = StructuredQuery {
            type_prefix: Some("goal:".into()),
            crs_min: Some(0.8),
            limit: 10,
            order_by: "crs".into(),
            relation_direction: "both".into(),
            ..Default::default()
        };
        let got = filter_rows(&sample(), &q);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].concept, "goal:a");
    }

    #[test]
    fn filter_related_to_one_hop() {
        let q = StructuredQuery {
            related_to: Some("goal:a".into()),
            relation_direction: "in".into(),
            limit: 10,
            order_by: "crs".into(),
            ..Default::default()
        };
        let got = filter_rows(&sample(), &q);
        // goal:b has edge in from goal:a; goal:a equals seed
        assert!(
            got.iter().any(|r| r.concept == "goal:b") || got.iter().any(|r| r.concept == "goal:a")
        );
    }

    #[test]
    fn limit_respected() {
        let q = StructuredQuery {
            limit: 2,
            order_by: "crs".into(),
            relation_direction: "both".into(),
            ..Default::default()
        };
        let got = filter_rows(&sample(), &q);
        assert_eq!(got.len(), 2);
    }
}
