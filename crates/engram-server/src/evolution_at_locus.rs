//! Bounded evolution bundle at a file locus — arcs, trace chain, scars.
//!
//! Reuses [`StoreHandle`] locus collectors from `context_for_edit` without a full atlas payload.

use crate::harness_injection;
use crate::store::StoreHandle;
use engram_core::storage;
use serde_json::{json, Value};
use std::collections::HashSet;

pub const MAX_LOCI: usize = 8;
pub const MAX_ARC_SEGMENTS: usize = 4;
pub const DEFAULT_PREVIEW_CHARS: usize = 200;
pub const DEFAULT_TRACE_DEPTH: usize = 6;
pub const VAR_CTX_PROGRAM_TRACES: &str = "var:ctx_program_traces";

pub struct EvolutionAtLocusParams<'a> {
    pub path: &'a str,
    pub line_start: Option<u32>,
    pub line_end: Option<u32>,
    pub preview_chars: usize,
    pub trace_depth: usize,
    /// When true (default), force-ingest file if loci empty — mirrors `context_for_edit`.
    pub auto_ingest: bool,
}

/// Parse `--- update @ {ts} ---` segments from arc provlog (chronological; capped to most recent).
pub fn parse_arc_segments(provlog: &str, preview_chars: usize, max_segments: usize) -> Vec<String> {
    const MARKER: &str = "--- update @";
    let trimmed = provlog.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let mut segments = Vec::new();
    if !trimmed.contains(MARKER) {
        segments.push(truncate_chars(trimmed, preview_chars));
        return segments;
    }

    let mut rest = trimmed;
    if let Some(idx) = rest.find(MARKER) {
        let seed = rest[..idx].trim();
        if !seed.is_empty() {
            segments.push(truncate_chars(seed, preview_chars));
        }
        rest = &rest[idx..];
    }

    for chunk in rest.split(MARKER).filter(|s| !s.trim().is_empty()) {
        let seg = format!("{MARKER}{chunk}");
        segments.push(truncate_chars(seg.trim_end(), preview_chars));
    }

    if segments.len() > max_segments {
        segments = segments[segments.len() - max_segments..].to_vec();
    }
    segments
}

fn truncate_chars(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max {
        return s.to_string();
    }
    format!(
        "{}…",
        s.chars().take(max.saturating_sub(1)).collect::<String>()
    )
}

fn arc_stability_from_block(block: &engram_core::types::Leg3Pointer) -> &'static str {
    if block.energetics.h_in < -0.01 {
        "converging"
    } else if block.energetics.dv > 0.35 {
        "in_flux"
    } else {
        "stable"
    }
}

fn traces_head_concept(traces: &[Value]) -> Option<String> {
    traces
        .first()
        .and_then(|v| v.get("concept").and_then(|c| c.as_str()).map(str::to_string))
}

fn walk_trace_chain(store: &StoreHandle, head: &str, depth: usize) -> Vec<Value> {
    let mut chain = Vec::new();
    let mut current = head.to_string();
    let mut seen = HashSet::new();

    for _ in 0..depth {
        if !seen.insert(current.clone()) {
            break;
        }

        let decision_point = store
            .trace_summary_at(&current)
            .and_then(|v| {
                v.get("decision_point")
                    .and_then(|d| d.as_str())
                    .map(str::to_string)
            })
            .unwrap_or_default();

        let prevs: Vec<String> = store
            .search_relations(&current, Some("prev_in_trace"), "to")
            .into_iter()
            .map(|(_, c)| c)
            .collect();
        let prev = prevs.first().cloned();

        chain.push(json!({
            "concept": current,
            "decision_point": decision_point,
            "prev": prev,
        }));

        match prev {
            Some(p) => current = p,
            None => break,
        }
    }
    chain
}

fn build_arcs_for_loci(
    store: &StoreHandle,
    loci: &[String],
    preview_chars: usize,
) -> Vec<Value> {
    loci.iter()
        .filter_map(|concept| {
            let arc_name = StoreHandle::arc_concept_name(concept);
            let block = store
                .fetch_block_high_priority(&arc_name)
                .or_else(|| store.fetch_block(&arc_name))?;
            let provlog = storage::read_provlog(&block);
            let segments = parse_arc_segments(&provlog, preview_chars, MAX_ARC_SEGMENTS);
            if segments.is_empty() {
                return None;
            }
            Some(json!({
                "concept": arc_name,
                "segments": segments,
                "drift_velocity": block.energetics.dv,
                "stability": arc_stability_from_block(&block),
            }))
        })
        .collect()
}

/// Build bounded evolution JSON for a file locus.
pub fn build_evolution_at_locus(
    store: &mut StoreHandle,
    params: EvolutionAtLocusParams<'_>,
) -> Value {
    let (stem, loci, ingest_performed) = store.spatial_loci_at_file(
        params.path,
        params.line_start,
        params.line_end,
        MAX_LOCI,
        params.auto_ingest,
    );

    let start_line = params.line_start.map(|l| l as f32).unwrap_or(0.0);
    let end_line = params.line_end.map(|l| l as f32).unwrap_or(999999.0);

    let tiers = store.collect_traces_at_locus(
        &stem,
        params.path,
        start_line,
        end_line,
        &loci,
        12,
    );

    let trace_chain = traces_head_concept(&tiers.line_precise)
        .or_else(|| traces_head_concept(&tiers.file_level))
        .or_else(|| traces_head_concept(&tiers.relation_linked))
        .map(|head| walk_trace_chain(store, &head, params.trace_depth))
        .unwrap_or_default();

    let scars_at_locus = store.collect_scars_at_locus(&stem, &loci, 8);
    let arcs = build_arcs_for_loci(store, &loci, params.preview_chars);
    let chain_summary_tile = harness_injection::latest_chain_summary_concept(store);

    let mut out = json!({
        "file_path": params.path,
        "loci": loci,
        "arcs": arcs,
        "trace_chain": trace_chain,
        "scars_at_locus": scars_at_locus,
        "chain_summary_tile": chain_summary_tile,
        "var_handles": [VAR_CTX_PROGRAM_TRACES],
    });

    if params.line_start.is_some() || params.line_end.is_some() {
        out["line_range"] = json!({
            "start": params.line_start,
            "end": params.line_end,
        });
    }

    if ingest_performed {
        out["ingest_performed"] = json!(true);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use engram_core::storage::{ProvlogSpliceMode, splice_provlog};

    fn test_store_dir(suffix: &str) -> std::path::PathBuf {
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        std::env::set_var("ENGRAM_FORCE_CPU_BACKEND", "1");
        std::env::set_var("ENGRAM_KI_DISABLE", "1");
        let dir = std::env::temp_dir().join(format!(
            "evolution_at_locus_{}_{}_{}",
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

    fn trace_body(spatial_context: &str, decision: &str) -> String {
        format!(
            "REASONING TRACE SEGMENT\n\n**decision_point:** {decision}\n\n**justification:** test\n\n**spatial_context:** {spatial_context}\n"
        )
    }

    fn seed_spatial_locus(
        store: &mut StoreHandle,
        concept: &str,
        line_start: i32,
        line_end: i32,
    ) {
        store
            .remember(concept, &format!("fn {concept}() {{}}"))
            .unwrap();
        let mut block = store.fetch_block(concept).unwrap();
        block.aabb_min[0] = line_start as f32;
        block.aabb_max[0] = line_end as f32;
        store.store(concept, block).unwrap();
    }

    #[test]
    fn parse_arc_segments_seed_only() {
        let segs = parse_arc_segments("EDIT ARC seed narrative", 80, 4);
        assert_eq!(segs.len(), 1);
        assert!(segs[0].contains("EDIT ARC"));
    }

    #[test]
    fn parse_arc_segments_with_updates() {
        let mut text = "EDIT ARC seed".to_string();
        text = splice_provlog(&text, "delta one", ProvlogSpliceMode::Append);
        text = splice_provlog(&text, "delta two", ProvlogSpliceMode::Append);

        let segs = parse_arc_segments(&text, 200, 4);
        assert!(segs.len() >= 2);
        assert!(segs.iter().any(|s| s.contains("--- update @")));
        assert!(segs.iter().any(|s| s.contains("delta two")));
    }

    #[test]
    fn parse_arc_segments_caps_at_max() {
        let mut text = "seed".to_string();
        for i in 0..6 {
            text = splice_provlog(&text, &format!("delta {i}"), ProvlogSpliceMode::Append);
        }
        let segs = parse_arc_segments(&text, 200, 4);
        assert_eq!(segs.len(), 4);
        assert!(segs.last().unwrap().contains("delta 5"));
    }

    #[test]
    fn evolution_trace_chain_walks_prev_in_trace() {
        let dir = test_store_dir("trace_chain");
        let mut store = StoreHandle::new(&dir.to_string_lossy());

        store
            .remember(
                "trace:evo_head",
                &trace_body("evo.rs:10", "head decision"),
            )
            .unwrap();
        store
            .remember(
                "trace:evo_prev",
                &trace_body("evo.rs:99", "older decision"),
            )
            .unwrap();
        store
            .relate("trace:evo_prev", "trace:evo_head", "prev_in_trace")
            .unwrap();

        seed_spatial_locus(&mut store, "evo__fn__foo", 5, 15);

        let out = build_evolution_at_locus(
            &mut store,
            EvolutionAtLocusParams {
                path: "/tmp/evo.rs",
                line_start: Some(8),
                line_end: Some(12),
                preview_chars: 200,
                trace_depth: 6,
                auto_ingest: true,
            },
        );

        let chain = out
            .get("trace_chain")
            .and_then(|v| v.as_array())
            .expect("trace_chain");
        assert_eq!(chain.len(), 2);
        assert_eq!(
            chain[0].get("concept").and_then(|v| v.as_str()),
            Some("trace:evo_head")
        );
        assert_eq!(
            chain[0].get("decision_point").and_then(|v| v.as_str()),
            Some("head decision")
        );
        assert_eq!(
            chain[0].get("prev").and_then(|v| v.as_str()),
            Some("trace:evo_prev")
        );
        assert_eq!(
            chain[1].get("concept").and_then(|v| v.as_str()),
            Some("trace:evo_prev")
        );
        assert_eq!(
            chain[1].get("decision_point").and_then(|v| v.as_str()),
            Some("older decision")
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn evolution_arcs_and_loci_from_spatial_window() {
        let dir = test_store_dir("arcs_loci");
        let mut store = StoreHandle::new(&dir.to_string_lossy());

        seed_spatial_locus(&mut store, "evo__fn__bar", 20, 40);
        store.ensure_edit_arc("evo__fn__bar").unwrap();
        let arc = StoreHandle::arc_concept_name("evo__fn__bar");
        store
            .update(&arc, "first delta narrative")
            .unwrap();
        store.update(&arc, "second delta narrative").unwrap();

        let out = build_evolution_at_locus(
            &mut store,
            EvolutionAtLocusParams {
                path: "/tmp/evo.rs",
                line_start: Some(25),
                line_end: Some(35),
                preview_chars: 200,
                trace_depth: 6,
                auto_ingest: true,
            },
        );

        let loci = out.get("loci").and_then(|v| v.as_array()).unwrap();
        assert_eq!(loci.len(), 1);
        assert_eq!(loci[0].as_str(), Some("evo__fn__bar"));

        let arcs = out.get("arcs").and_then(|v| v.as_array()).unwrap();
        assert_eq!(arcs.len(), 1);
        assert_eq!(arcs[0].get("concept").and_then(|v| v.as_str()), Some(arc.as_str()));
        let segments = arcs[0]
            .get("segments")
            .and_then(|v| v.as_array())
            .unwrap();
        assert!(!segments.is_empty());
        assert!(segments.iter().any(|s| s.as_str().unwrap().contains("--- update @")));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn evolution_includes_var_handles_and_chain_summary() {
        let dir = test_store_dir("var_chain");
        let mut store = StoreHandle::new(&dir.to_string_lossy());

        store
            .remember(
                "tile:chain_summary_test_abc",
                "THOUGHT TILE\n\n**tile_type:** chain_summary\n",
            )
            .unwrap();

        let out = build_evolution_at_locus(
            &mut store,
            EvolutionAtLocusParams {
                path: "/tmp/empty.rs",
                line_start: None,
                line_end: None,
                preview_chars: 200,
                trace_depth: 6,
                auto_ingest: true,
            },
        );

        let handles = out
            .get("var_handles")
            .and_then(|v| v.as_array())
            .unwrap();
        assert_eq!(handles.len(), 1);
        assert_eq!(handles[0].as_str(), Some(VAR_CTX_PROGRAM_TRACES));

        assert_eq!(
            out.get("chain_summary_tile").and_then(|v| v.as_str()),
            Some("tile:chain_summary_test_abc")
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn evolution_stem_prefix_resolves_loci_without_hot_sample() {
        let dir = test_store_dir("stem_prefix");
        let mut store = StoreHandle::new(&dir.to_string_lossy());

        seed_spatial_locus(&mut store, "evo__fn__target_fn", 5, 15);
        store.ensure_edit_arc("evo__fn__target_fn").unwrap();
        store
            .update(
                &StoreHandle::arc_concept_name("evo__fn__target_fn"),
                "delta from stem-prefix resolution test",
            )
            .unwrap();

        let out = build_evolution_at_locus(
            &mut store,
            EvolutionAtLocusParams {
                path: "/tmp/evo.rs",
                line_start: Some(8),
                line_end: Some(12),
                preview_chars: 200,
                trace_depth: 6,
                auto_ingest: false,
            },
        );

        let loci = out.get("loci").and_then(|v| v.as_array()).unwrap();
        assert_eq!(loci.len(), 1);
        assert_eq!(loci[0].as_str(), Some("evo__fn__target_fn"));

        let arcs = out.get("arcs").and_then(|v| v.as_array()).unwrap();
        assert_eq!(arcs.len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }
}