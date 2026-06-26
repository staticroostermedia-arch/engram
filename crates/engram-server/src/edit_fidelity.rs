//! Agent tool fidelity — safe edit sequences, lineage verification, tensor-backed edit patterns.
//!
//! Composite MCP tools (`safe_edit_and_verify`, `update_with_tensor_bond`) and harness
//! reflection loops call these helpers. Keeps dispatch thin and testable.

use crate::solid_state_tensor::{tensor_upsert, BondSpec};
use crate::store::{ManifoldVerificationOptions, StoreHandle};
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

/// Lineage report for edit/update composite tools.
#[derive(Debug, Clone)]
pub struct LineageReport {
    pub ok: bool,
    pub trace_id: Option<String>,
    pub arc_concept: Option<String>,
    pub issues: Vec<String>,
    pub crs_before: Option<f32>,
    pub crs_after: Option<f32>,
}

impl LineageReport {
    pub fn to_json(&self) -> Value {
        let crs_delta = match (self.crs_before, self.crs_after) {
            (Some(b), Some(a)) => Some(a - b),
            _ => None,
        };
        json!({
            "ok": self.ok,
            "trace_id": self.trace_id,
            "arc_concept": self.arc_concept,
            "issues": self.issues,
            "crs_before": self.crs_before,
            "crs_after": self.crs_after,
            "crs_delta": crs_delta,
        })
    }
}

/// Verify trace + optional arc blocks exist with CRS >= min_crs and optional prev chain.
pub fn verify_edit_lineage(
    store: &StoreHandle,
    trace_id: Option<&str>,
    arc_concept: Option<&str>,
    prev_trace: Option<&str>,
    min_crs: f32,
) -> LineageReport {
    let mut issues = Vec::new();
    let mut crs_before = None;
    let mut crs_after = None;

    if let Some(tid) = trace_id {
        if let Some(block) = store.fetch_block_high_priority(tid) {
            crs_after = Some(block.crs_score);
            if block.crs_score < min_crs {
                issues.push(format!(
                    "trace {tid} CRS {:.3} < {min_crs}",
                    block.crs_score
                ));
            }
            if store.fetch_block(tid).is_none() {
                issues.push(format!("trace {tid} not in manifold index"));
            }
        } else {
            issues.push(format!("trace {tid} not found"));
        }
    } else {
        issues.push("no trace_id minted".to_string());
    }

    if let Some(arc) = arc_concept {
        if let Some(block) = store.fetch_block(arc) {
            crs_before = crs_before.or(Some(block.crs_score));
            if block.crs_score < min_crs {
                issues.push(format!("arc {arc} CRS {:.3} < {min_crs}", block.crs_score));
            }
        } else {
            issues.push(format!("arc {arc} not found"));
        }
    }

    if let (Some(prev), Some(tid)) = (prev_trace, trace_id) {
        if !prev.is_empty() {
            let chained = store
                .search_relations(prev, Some("prev_in_trace"), "to")
                .iter()
                .any(|(_label, other)| other == tid);
            if !chained {
                issues.push(format!("prev_in_trace chain missing: {prev} → {tid}"));
            }
        }
    }

    LineageReport {
        ok: issues.is_empty(),
        trace_id: trace_id.map(str::to_string),
        arc_concept: arc_concept.map(str::to_string),
        issues,
        crs_before,
        crs_after,
    }
}

fn timestamp_slug() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn slugify(s: &str, max: usize) -> String {
    s.chars()
        .take(max)
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

/// Mint a quick_trace block; returns trace concept name.
pub fn mint_quick_trace(
    store: &mut StoreHandle,
    decision: &str,
    why: &str,
    spatial_context: Option<&str>,
    prev_trace: Option<&str>,
    goal_context: Option<&str>,
) -> Result<String, String> {
    let mut prev = prev_trace.unwrap_or("").trim().to_string();
    if prev.is_empty() {
        if let Some(head) = store.latest_trace_head() {
            prev = head;
        }
    }

    let timestamp = timestamp_slug();
    let payload = format!(
        "REASONING TRACE SEGMENT (via safe_edit_and_verify)\n\n**decision_point:** {}\n\n**justification:** {}\n",
        decision, why
    );
    let mut trace_block = store.encode(&payload);
    trace_block.zedos_tag = engram_core::types::ZEDOS_TRAINING;
    trace_block.crs_score = 0.85;
    crate::store::assign_reflexive_contract(&mut trace_block);
    trace_block.energetics.ts = timestamp;
    trace_block.energetics.crs = trace_block.crs_score;

    let short = slugify(decision, 48);
    let trace_key = format!("trace:{}_{}", timestamp, short);

    store
        .store(&trace_key, trace_block)
        .map_err(|e| format!("trace store: {e}"))?;

    if !prev.is_empty() {
        let _ = store.relate(&prev, &trace_key, "prev_in_trace");
        let _ = store.relate(&trace_key, &prev, "next_in_trace");
    }
    if let Some(sp) = spatial_context {
        if !sp.is_empty() {
            let _ = store.wire_trace_to_spatial_locus(&trace_key, sp);
        }
    }
    if let Some(goal) = goal_context {
        if !goal.is_empty() {
            let _ = store.relate(&trace_key, goal, "serves");
        }
    }

    Ok(trace_key)
}

/// Upsert tensor pattern for edit success/failure; bonds to locus + trace.
pub fn tensor_pattern_for_edit(
    store: &mut StoreHandle,
    success: bool,
    locus: &str,
    trace_id: Option<&str>,
    note: &str,
) -> anyhow::Result<Value> {
    let ts = timestamp_slug();
    let kind = if success { "success" } else { "failure" };
    let stem = slugify(locus, 32);
    let concept = format!("tensor:edit_pattern_{kind}_{stem}_{ts}");
    let text = format!(
        "edit_fidelity pattern ({kind})\nlocus: {locus}\ntrace: {}\nnote: {note}",
        trace_id.unwrap_or("(none)")
    );

    let mut bonds = vec![BondSpec {
        from: concept.clone(),
        to: locus.to_string(),
        label: "edit_fidelity".to_string(),
    }];
    if let Some(tid) = trace_id {
        bonds.push(BondSpec {
            from: concept.clone(),
            to: tid.to_string(),
            label: "edit_fidelity".to_string(),
        });
    }

    let result = tensor_upsert(store, &concept, &text, &bonds, true)?;
    Ok(json!({
        "concept": result.concept,
        "stored": result.stored,
        "promoted": result.promoted,
        "bonds_created": result.bonds_created.len(),
    }))
}

/// Reflection loop actions suggested after edit/update (harness_injection palette).
pub fn build_reflection_loop_actions(
    trace_id: Option<&str>,
    arc_concept: Option<&str>,
    path: Option<&str>,
) -> Vec<Value> {
    let mut actions = vec![
        json!({
            "tool": "mcp_engram_quick_trace",
            "reason": "reflection: post-edit delta trace (chain prev)",
            "priority": 1,
            "args_hint": {
                "decision": "Post-edit verification complete",
                "why": "Record delta after safe edit or update",
                "prev": trace_id.unwrap_or("trace_chain.head"),
            },
        }),
        json!({
            "tool": "mcp_engram_verify_block_lawfulness",
            "reason": "reflection: CRS gate on edited locus",
            "priority": 2,
            "args_hint": {
                "concept": arc_concept.unwrap_or("{ast_concept}__arc"),
            },
        }),
    ];
    if path.is_some() {
        actions.push(json!({
            "tool": "mcp_engram_tensor_upsert",
            "reason": "reflection: record edit pattern in solid-state tensor",
            "priority": 3,
            "args_hint": {
                "concept": "tensor:edit_pattern_success_{stem}",
                "text": "pattern: correct edit sequence at locus",
                "bonds": [{"from": "tensor:edit_pattern_*", "to": trace_id, "label": "edit_fidelity"}],
            },
        }));
    }
    actions
}

/// First non-arc spatial concept from context_for_edit payload.
pub fn first_arc_from_context(payload: &Value) -> Option<String> {
    payload
        .get("spatial_items")
        .and_then(|v| v.as_array())
        .and_then(|items| {
            items.iter().find_map(|item| {
                item.get("concept")
                    .and_then(|c| c.as_str())
                    .filter(|c| !c.ends_with("__arc"))
                    .map(|c| StoreHandle::arc_concept_name(c))
            })
        })
}

/// Composite safe edit: context → trace → optional arc update → verify → lineage → tensor pattern.
pub fn run_safe_edit_and_verify(
    store: &mut StoreHandle,
    path: &str,
    decision: &str,
    why: &str,
    arc_delta: Option<&str>,
    prev_trace: Option<&str>,
    goal_context: Option<&str>,
    run_verify: bool,
) -> Value {
    let context = store.context_for_edit(path, None, None, true);
    let _ = crate::edit_arc_gate::register_from_context(path, &context);
    let spatial_ctx = path.to_string();

    let trace_id = match mint_quick_trace(
        store,
        decision,
        why,
        Some(&spatial_ctx),
        prev_trace,
        goal_context,
    ) {
        Ok(t) => Some(t),
        Err(e) => {
            return json!({
                "ok": false,
                "error": "trace_mint_failed",
                "detail": e,
                "path": path,
            });
        }
    };

    let arc_concept = first_arc_from_context(&context);
    let mut arc_updated = false;
    if let (Some(arc), Some(delta)) = (arc_concept.as_deref(), arc_delta) {
        if !delta.trim().is_empty() {
            if store.update(arc, delta).is_ok() {
                crate::edit_arc_gate::on_arc_updated(arc);
                arc_updated = true;
            }
        }
    }

    let verify_pass = if run_verify {
        let opts = ManifoldVerificationOptions {
            min_crs: 0.74,
            sample_size: Some(32),
            include_relation_integrity: false,
        };
        store
            .verify_manifold_integrity(opts)
            .map(|r| r.overall_health == "healthy" || r.issues_found == 0)
            .unwrap_or(false)
    } else {
        true
    };

    let lineage = verify_edit_lineage(
        store,
        trace_id.as_deref(),
        arc_concept.as_deref(),
        prev_trace,
        0.74,
    );

    let pattern_note = if lineage.ok && verify_pass {
        "safe_edit_and_verify succeeded"
    } else {
        "safe_edit_and_verify partial — check lineage issues"
    };
    let tensor_pattern = tensor_pattern_for_edit(
        store,
        lineage.ok && verify_pass,
        path,
        trace_id.as_deref(),
        pattern_note,
    )
    .ok();

    let reflection =
        build_reflection_loop_actions(trace_id.as_deref(), arc_concept.as_deref(), Some(path));

    json!({
        "ok": lineage.ok && verify_pass,
        "path": path,
        "trace_id": trace_id,
        "arc_concept": arc_concept,
        "arc_updated": arc_updated,
        "verify_pass": verify_pass,
        "lineage": lineage.to_json(),
        "tensor_pattern": tensor_pattern,
        "reflection_suggested": reflection,
        "context_spatial_count": context.get("spatial_items").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0),
    })
}

/// Recall-first update with tensor bond; scars on mismatch when enabled.
pub fn run_update_with_tensor_bond(
    store: &mut StoreHandle,
    concept: &str,
    new_text: &str,
    recall_query: Option<&str>,
    bond_label: &str,
    scar_on_mismatch: bool,
    match_threshold: f32,
) -> Value {
    let concept = concept.trim();
    let new_text = new_text.trim();
    if concept.is_empty() || new_text.is_empty() {
        return json!({
            "ok": false,
            "error": "concept and new_text required",
        });
    }

    let mut recall_match = true;
    let mut top_concept = None;
    let mut top_score = 0.0_f32;

    if let Some(q) = recall_query {
        if !q.trim().is_empty() {
            let (hits, _) = store.recall_scoped(q, 5, Some("anchors"));
            if let Some(top) = hits.first() {
                top_score = top.score;
                top_concept = Some(top.concept.clone());
                let name_match = top.concept == concept || top.concept.contains(concept);
                recall_match = name_match || top.score >= match_threshold;
            } else {
                recall_match = store.fetch_block(concept).is_some();
            }
        }
    }

    let crs_before = store
        .fetch_block(concept)
        .map(|b| b.crs_score)
        .unwrap_or(1.0);

    if !recall_match && scar_on_mismatch {
        let ts = timestamp_slug();
        let scar_key = format!("scar:update_mismatch_{ts}");
        let scar_text = format!(
            "SCAR: update_with_tensor_bond recall mismatch. target={concept} top={:?} score={top_score:.3}. Use recall first; prefer update over forget+remember.",
            top_concept
        );
        let mut block = store.encode(&scar_text);
        block.zedos_tag = engram_core::types::ZEDOS_PRAXIS;
        block.crs_score = 0.92;
        let _ = store.store(&scar_key, block);
    }

    let update_message = match store.update(concept, new_text) {
        Ok(msg) => msg,
        Err(e) => {
            return json!({
                "ok": false,
                "error": format!("update failed: {e}"),
                "recall_match": recall_match,
            });
        }
    };

    if concept.ends_with("__arc") {
        crate::edit_arc_gate::on_arc_updated(concept);
    }

    let crs_after = store
        .fetch_block(concept)
        .map(|b| b.crs_score)
        .unwrap_or(crs_before);

    let bond_lbl = if bond_label.is_empty() {
        "edit_fidelity"
    } else {
        bond_label
    };
    let pattern_concept = format!(
        "tensor:update_pattern_{}_{}",
        slugify(concept, 24),
        timestamp_slug()
    );
    let pattern_text = format!(
        "verified_memory_update\nconcept: {concept}\nrecall_match: {recall_match}\ntop_score: {top_score:.3}\ndelta_len: {}",
        new_text.len()
    );
    let bonds = vec![
        BondSpec {
            from: pattern_concept.clone(),
            to: concept.to_string(),
            label: bond_lbl.to_string(),
        },
        BondSpec {
            from: pattern_concept.clone(),
            to: concept.to_string(),
            label: "updated_via".to_string(),
        },
    ];
    let tensor_result = tensor_upsert(store, &pattern_concept, &pattern_text, &bonds, true);

    json!({
        "ok": true,
        "concept": concept,
        "message": update_message,
        "recall_match": recall_match,
        "recall_top": top_concept,
        "recall_top_score": top_score,
        "crs_before": crs_before,
        "crs_after": crs_after,
        "crs_delta": crs_after - crs_before,
        "tensor_pattern": tensor_result.ok().map(|r| json!({
            "concept": r.concept,
            "bonds": r.bonds_created.len(),
        })),
        "lineage": {
            "arc_cleared": concept.ends_with("__arc"),
            "update_only_mutation": true,
        },
        "reflection_suggested": build_reflection_loop_actions(None, Some(concept), None),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::StoreHandle;

    fn test_store() -> StoreHandle {
        let dir = std::env::temp_dir().join(format!(
            "engram-edit-fidelity-{}-{}",
            std::process::id(),
            timestamp_slug()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        StoreHandle::new(&dir.to_string_lossy())
    }

    #[test]
    fn verify_lineage_requires_trace() {
        let store = test_store();
        let r = verify_edit_lineage(&store, None, None, None, 0.74);
        assert!(!r.ok);
        assert!(r.issues.iter().any(|i| i.contains("trace")));
    }

    #[test]
    fn mint_quick_trace_creates_trace() {
        let mut store = test_store();
        let tid = mint_quick_trace(
            &mut store,
            "Test decision",
            "Because harness",
            Some("test.rs:1"),
            None,
            None,
        )
        .expect("mint");
        assert!(tid.starts_with("trace:"));
        let r = verify_edit_lineage(&store, Some(&tid), None, None, 0.74);
        assert!(r.ok, "{:?}", r.issues);
    }

    #[test]
    fn tensor_pattern_for_edit_bonds() {
        let mut store = test_store();
        let locus = "store__fn__harness_test";
        store
            .remember(locus, "harness locus stub")
            .expect("remember locus");
        let tid = mint_quick_trace(&mut store, "d", "w", None, None, None).unwrap();
        let p =
            tensor_pattern_for_edit(&mut store, true, locus, Some(&tid), "ok").expect("pattern");
        assert!(p
            .get("concept")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .contains("edit_pattern_success"));
    }
}
