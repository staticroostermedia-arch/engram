//! Agent tool fidelity — safe edit sequences, lineage verification, tensor-backed edit patterns.

use crate::solid_state_tensor::{tensor_upsert, BondSpec};
use crate::store::{ManifoldVerificationOptions, StoreHandle};
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

const MIN_CRS: f32 = 0.74;

/// Lineage report for edit/update composite tools.
#[derive(Debug, Clone)]
pub struct LineageReport {
    pub ok: bool,
    pub trace_id: Option<String>,
    pub arc_concept: Option<String>,
    pub issues: Vec<String>,
    pub crs_before: Option<f32>,
    pub crs_after: Option<f32>,
    pub merkle_ok: bool,
    pub merkle_trace_sig: Option<String>,
    pub merkle_arc_sig: Option<String>,
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
            "merkle_ok": self.merkle_ok,
            "merkle_trace_sig": self.merkle_trace_sig,
            "merkle_arc_sig": self.merkle_arc_sig,
        })
    }
}

fn merkle_preview(sig: &[u8; 32]) -> String {
    sig.iter().take(4).map(|b| format!("{b:02x}")).collect()
}

fn block_merkle_lineage_ok(store: &StoreHandle, concept: &str) -> (bool, Option<String>) {
    let block = store
        .fetch_block_high_priority(concept)
        .or_else(|| store.fetch_block(concept));
    let Some(block) = block else {
        return (false, None);
    };
    // sig_0 = BLAKE3 footer anchor (verify_block_lawfulness); merkle_sub_root on relations/updates
    let sig_ok = block.footer.sig_0.iter().any(|&b| b != 0);
    (sig_ok, Some(merkle_preview(&block.footer.sig_0)))
}

/// Verify trace + optional arc blocks: CRS >= min_crs, optional prev chain, merkle footer.
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
    let mut merkle_trace_sig = None;
    let mut merkle_arc_sig = None;
    let mut merkle_ok = true;

    if let Some(tid) = trace_id {
        if let Some(block) = store.fetch_block_high_priority(tid) {
            crs_after = Some(block.crs_score);
            if block.crs_score < min_crs {
                issues.push(format!(
                    "trace {tid} CRS {:.3} < {min_crs}",
                    block.crs_score
                ));
            }
            let (m_ok, m_sig) = block_merkle_lineage_ok(store, tid);
            merkle_trace_sig = m_sig;
            if !m_ok {
                merkle_ok = false;
                issues.push(format!("trace {tid} missing merkle/sig_0 footer"));
            }
        } else {
            issues.push(format!("trace {tid} not found"));
            merkle_ok = false;
        }
    } else {
        issues.push("no trace_id minted".to_string());
        merkle_ok = false;
    }

    if let Some(arc) = arc_concept {
        if let Some(block) = store.fetch_block(arc) {
            crs_before = crs_before.or(Some(block.crs_score));
            if block.crs_score < min_crs {
                issues.push(format!("arc {arc} CRS {:.3} < {min_crs}", block.crs_score));
            }
            let (m_ok, m_sig) = block_merkle_lineage_ok(store, arc);
            merkle_arc_sig = m_sig;
            if !m_ok {
                merkle_ok = false;
                issues.push(format!("arc {arc} missing merkle/sig_0 footer"));
            }
        } else {
            issues.push(format!("arc {arc} not found"));
            merkle_ok = false;
        }
    }

    if let (Some(prev), Some(tid)) = (prev_trace, trace_id) {
        if !prev.is_empty() {
            // Edge stored as prev --prev_in_trace--> tid (from=prev, to=tid)
            let chained = store
                .search_relations(prev, Some("prev_in_trace"), "from")
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
        merkle_ok,
        merkle_trace_sig,
        merkle_arc_sig,
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

    if store.fetch_block(locus).is_none() {
        store
            .remember(locus, &format!("edit_fidelity locus stub: {locus}"))
            .map_err(|e| anyhow::anyhow!("locus remember failed: {e}"))?;
    }

    let mut bonds = vec![BondSpec {
        from: concept.clone(),
        to: locus.to_string(),
        label: "edit_fidelity".to_string(),
    }];
    if let Some(tid) = trace_id {
        if store.fetch_block(tid).is_none() {
            store
                .remember(tid, &format!("edit_fidelity trace stub: {tid}"))
                .map_err(|e| anyhow::anyhow!("trace stub remember failed: {e}"))?;
        }
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
        "kind": kind,
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
            "reason": "reflection: CRS + merkle gate on edited locus",
            "priority": 2,
            "args_hint": {
                "concept": arc_concept.unwrap_or("{ast_concept}__arc"),
                "check_merkle_chain": true,
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

/// First non-arc spatial AST concept from context_for_edit payload.
pub fn first_ast_from_context(payload: &Value) -> Option<String> {
    payload
        .get("spatial_items")
        .and_then(|v| v.as_array())
        .and_then(|items| {
            items.iter().find_map(|item| {
                item.get("concept")
                    .and_then(|c| c.as_str())
                    .filter(|c| !c.is_empty() && !c.ends_with("__arc"))
                    .map(str::to_string)
            })
        })
}

fn ast_rank(concept: &str) -> u8 {
    if concept.contains("__fn__") {
        0
    } else if concept.contains("__mod__") {
        1
    } else if concept.contains("__struct__") {
        2
    } else if concept.contains("__enum__") {
        4
    } else {
        3
    }
}

/// Primary file locus for arc delta — prefer stem-matched fn/mod over first enum in spatial_items.
pub fn primary_ast_from_context(path: &str, payload: &Value) -> Option<String> {
    let stem = payload
        .get("stem")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            std::path::Path::new(path)
                .file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_string)
        })?;

    let prefix = format!("{stem}__");
    let mut candidates: Vec<String> = payload
        .get("spatial_items")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item.get("concept")
                        .and_then(|c| c.as_str())
                        .filter(|c| {
                            !c.is_empty() && !c.ends_with("__arc") && c.starts_with(&prefix)
                        })
                        .map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default();

    if candidates.is_empty() {
        return first_ast_from_context(payload);
    }
    candidates.sort_by_key(|c| ast_rank(c));
    candidates.into_iter().next()
}

/// Composite safe edit: context → trace → optional arc update → verify → lineage → tensor pattern.
#[allow(clippy::too_many_arguments)] // mirrors MCP tool schema fields
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

    let ast_concept = primary_ast_from_context(path, &context);
    let mut arc_concept = ast_concept
        .as_ref()
        .map(|a| StoreHandle::arc_concept_name(a));
    let mut arc_updated = false;
    let mut arc_update_error: Option<String> = None;

    if let Some(delta) = arc_delta {
        if !delta.trim().is_empty() {
            if let Some(ast) = ast_concept.as_deref() {
                match store.ensure_edit_arc(ast) {
                    Ok(arc) => {
                        arc_concept = Some(arc.clone());
                        match store.update(&arc, delta) {
                            Ok(_) => {
                                crate::edit_arc_gate::on_arc_updated(&arc);
                                arc_updated = true;
                            }
                            Err(e) => arc_update_error = Some(format!("arc update failed: {e}")),
                        }
                    }
                    Err(e) => arc_update_error = Some(format!("ensure_edit_arc failed: {e}")),
                }
            } else {
                arc_update_error =
                    Some("no spatial AST locus in context — cannot ensure __arc".to_string());
            }
        }
    }

    let verify_pass = if run_verify {
        let opts = ManifoldVerificationOptions {
            min_crs: MIN_CRS,
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
        MIN_CRS,
    );

    let success = lineage.ok && verify_pass && arc_update_error.is_none();
    let pattern_note = if success {
        "safe_edit_and_verify succeeded"
    } else {
        "safe_edit_and_verify partial — check lineage/arc issues"
    };
    let pattern_locus = arc_concept
        .as_deref()
        .or(ast_concept.as_deref())
        .unwrap_or(path);
    let tensor_pattern = match tensor_pattern_for_edit(
        store,
        success,
        pattern_locus,
        trace_id.as_deref(),
        pattern_note,
    ) {
        Ok(v) => Some(v),
        Err(e) => Some(json!({
            "kind": if success { "success" } else { "failure" },
            "concept": format!("tensor:edit_pattern_{}_{}", if success { "success" } else { "failure" }, slugify(pattern_locus, 24)),
            "error": e.to_string(),
            "bonds_created": 0,
        })),
    };

    let reflection =
        build_reflection_loop_actions(trace_id.as_deref(), arc_concept.as_deref(), Some(path));

    json!({
        "ok": success,
        "path": path,
        "trace_id": trace_id,
        "arc_concept": arc_concept,
        "arc_updated": arc_updated,
        "arc_update_error": arc_update_error,
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
    let mut scar_key: Option<String> = None;
    let mut failure_pattern: Option<Value> = None;

    if let Some(q) = recall_query {
        let q_trim = q.trim();
        if !q_trim.is_empty() {
            recall_match = false;
            if q_trim == concept {
                recall_match = true;
            } else if let Some(arc_base) = concept.strip_suffix("__arc") {
                if q_trim == arc_base || q_trim == concept {
                    recall_match = true;
                }
            }
            if !recall_match {
                let (hits, _) = store.recall_scoped(q, 5, Some("anchors"));
                if let Some(top) = hits.first() {
                    top_score = top.score;
                    top_concept = Some(top.concept.clone());
                    let name_match = top.concept == concept
                        || top.concept.contains(concept)
                        || concept.contains(top.concept.as_str());
                    let arc_pair_match = concept
                        .ends_with("__arc")
                        .then(|| {
                            concept.strip_suffix("__arc").map(|base| {
                                top.concept == base
                                    || base.starts_with(top.concept.as_str())
                                    || top.concept.starts_with(base)
                            })
                        })
                        .flatten()
                        .unwrap_or(false);
                    recall_match = name_match || arc_pair_match || top.score >= match_threshold;
                } else {
                    // Query provided but no hits — treat as mismatch (do not auto-pass because block exists)
                    recall_match = false;
                }
            }
        }
    }

    let crs_before = store
        .fetch_block(concept)
        .map(|b| b.crs_score)
        .unwrap_or(1.0);

    if !recall_match {
        if scar_on_mismatch {
            let ts = timestamp_slug();
            let key = format!("scar:update_mismatch_{ts}");
            let scar_text = format!(
                "SCAR: update_with_tensor_bond recall mismatch. target={concept} top={:?} score={top_score:.3}. Use recall first; prefer update over forget+remember.",
                top_concept
            );
            let mut block = store.encode(&scar_text);
            block.zedos_tag = engram_core::types::ZEDOS_PRAXIS;
            block.crs_score = 0.92;
            if store.store(&key, block).is_ok() {
                scar_key = Some(key);
            }
        }
        failure_pattern = Some(
            tensor_pattern_for_edit(
                store,
                false,
                concept,
                None,
                "update_with_tensor_bond recall mismatch",
            )
            .unwrap_or_else(|e| {
                json!({
                    "kind": "failure",
                    "concept": format!("tensor:edit_pattern_failure_{}", slugify(concept, 24)),
                    "error": e.to_string(),
                })
            }),
        );
    }

    let update_message = match store.update(concept, new_text) {
        Ok(msg) => msg,
        Err(e) => {
            return json!({
                "ok": false,
                "error": format!("update failed: {e}"),
                "recall_match": recall_match,
                "scar_key": scar_key,
                "failure_pattern": failure_pattern,
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

    let crs_gate_ok = crs_after >= MIN_CRS;
    if !crs_gate_ok {
        let ts = timestamp_slug();
        let key = format!("scar:crs_gate_fail_{ts}");
        let text = format!(
            "SCAR: update_with_tensor_bond CRS gate fail. concept={concept} crs_after={crs_after:.3} < {MIN_CRS}"
        );
        let mut block = store.encode(&text);
        block.zedos_tag = engram_core::types::ZEDOS_PRAXIS;
        block.crs_score = 0.92;
        if store.store(&key, block).is_ok() {
            scar_key = scar_key.or(Some(key));
        }
    }

    let bond_lbl = if bond_label.is_empty() {
        "edit_fidelity"
    } else {
        bond_label
    };
    let tensor_result = if recall_match && crs_gate_ok {
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
        tensor_upsert(store, &pattern_concept, &pattern_text, &bonds, true).ok()
    } else {
        None
    };

    let lineage = verify_edit_lineage(store, None, Some(concept), None, MIN_CRS);

    json!({
        "ok": recall_match && crs_gate_ok,
        "concept": concept,
        "message": update_message,
        "recall_match": recall_match,
        "recall_top": top_concept,
        "recall_top_score": top_score,
        "crs_before": crs_before,
        "crs_after": crs_after,
        "crs_delta": crs_after - crs_before,
        "crs_gate_ok": crs_gate_ok,
        "scar_key": scar_key,
        "failure_pattern": failure_pattern,
        "tensor_pattern": tensor_result.map(|r| json!({
            "concept": r.concept,
            "bonds": r.bonds_created.len(),
            "stored": r.stored,
        })),
        "lineage": lineage.to_json(),
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
    fn verify_lineage_requires_trace_and_merkle() {
        let store = test_store();
        let r = verify_edit_lineage(&store, None, None, None, MIN_CRS);
        assert!(!r.ok);
        assert!(!r.merkle_ok);
    }

    #[test]
    fn mint_quick_trace_has_merkle() {
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
        let r = verify_edit_lineage(&store, Some(&tid), None, None, MIN_CRS);
        assert!(r.ok, "{:?}", r.issues);
        assert!(r.merkle_ok);
        assert!(r.merkle_trace_sig.is_some());
    }

    #[test]
    fn ensure_edit_arc_before_delta() {
        let mut store = test_store();
        let ast = "store__fn__harness_arc_test";
        store.remember(ast, "fn stub").expect("remember ast");
        let arc = store.ensure_edit_arc(ast).expect("ensure arc");
        assert!(store.update(&arc, "delta: harness arc test").is_ok());
        let (merkle_ok, _) = block_merkle_lineage_ok(&store, &arc);
        assert!(merkle_ok, "arc block should have sig_0 footer");
    }

    #[test]
    fn tensor_pattern_for_edit_bonds() {
        let mut store = test_store();
        let locus = "store__fn__harness_test";
        seed_grounded_block(&mut store, locus, "harness locus stub");
        let tid = mint_quick_trace(&mut store, "d", "w", None, None, None).unwrap();
        let p =
            tensor_pattern_for_edit(&mut store, true, locus, Some(&tid), "ok").expect("pattern");
        assert!(p
            .get("concept")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .contains("edit_pattern_success"));
    }

    #[test]
    fn primary_ast_prefers_fn_over_enum() {
        let payload = json!({
            "stem": "profile",
            "spatial_items": [
                {"concept": "profile__enum__engramprofile"},
                {"concept": "profile__fn__main"},
            ]
        });
        let ast = primary_ast_from_context("/tmp/profile.rs", &payload).unwrap();
        assert!(ast.contains("__fn__"), "got {ast}");
    }

    fn seed_grounded_block(store: &mut StoreHandle, concept: &str, text: &str) {
        store.remember(concept, text).expect("remember");
        if let Some(mut block) = store.fetch_block(concept) {
            block.crs_score = 0.85;
            store.store(concept, block).expect("store crs bump");
        }
    }

    #[test]
    fn arc_pair_recall_match() {
        let mut store = test_store();
        let base = "harness:edit_fidelity_test";
        let arc = format!("{base}__arc");
        seed_grounded_block(&mut store, base, "harness base");
        seed_grounded_block(&mut store, &arc, "harness arc");
        let out = run_update_with_tensor_bond(
            &mut store,
            &arc,
            "delta: arc update",
            Some(base),
            "edit_fidelity",
            false,
            0.85,
        );
        assert_eq!(
            out.get("recall_match"),
            Some(&json!(true)),
            "recall_match failed: {out}"
        );
        assert_eq!(out.get("ok"), Some(&json!(true)), "ok failed: {out}");
    }

    #[test]
    fn prev_in_trace_chain_verified() {
        let mut store = test_store();
        let prev =
            mint_quick_trace(&mut store, "prev", "harness chain", None, None, None).expect("prev");
        let next =
            mint_quick_trace(&mut store, "next", "chained", None, Some(&prev), None).expect("next");
        let r = verify_edit_lineage(&store, Some(&next), None, Some(&prev), MIN_CRS);
        assert!(r.ok, "prev_in_trace chain should verify: {:?}", r.issues);
        assert!(r.merkle_ok);
    }
}
