//! Context variable handles — extend context window without unpacking full provlog.
//!
//! Generalizes `LinguisticDiscourseBundle` to `ContextBundle`: slots reference manifold
//! concepts + on-disk geometry; agents manipulate handles via declare/query/project.

use crate::evolution_at_locus::VAR_CTX_PROGRAM_TRACES;
use crate::store::StoreHandle;
use engram_core::storage;
use engram_core::types::{
    LinguisticContextPatch, LinguisticDiscourseBundle, LinguisticWord, ZEDOS_OPERATIONAL,
};
use serde_json::{json, Value};
use std::collections::HashSet;

pub const CONTEXT_BUNDLE_FORMAT: &str = "context_bundle_v1";
pub const PROGRAM_TRACE_PREVIEW_CHARS: usize = 150;
pub const MAX_PROGRAM_TRACE_SLOTS: usize = 8;

#[derive(Debug, Clone)]
pub struct ContextSlot {
    pub concept: String,
    pub geometry_ref: String,
    pub crs: f32,
    pub zedos_tag: u8,
    pub preview: String,
    pub relation_count: usize,
}

#[derive(Debug, Clone)]
pub struct ContextBundle {
    pub bundle_id: String,
    pub slots: Vec<ContextSlot>,
    pub patches: Vec<LinguisticContextPatch>,
    pub functor_metadata: String,
    pub created_at: u64,
}

pub fn normalize_var_name(name: &str) -> String {
    let n = name.trim();
    if n.is_empty() {
        return String::new();
    }
    if n.starts_with("var:") {
        n.to_string()
    } else {
        format!("var:{n}")
    }
}

fn preview_text(text: &str, max_chars: usize) -> String {
    let p: String = text.chars().take(max_chars).collect();
    if text.chars().count() > max_chars {
        format!("{p}…")
    } else {
        p
    }
}

pub fn bundle_to_json(bundle: &ContextBundle) -> Value {
    json!({
        "format": CONTEXT_BUNDLE_FORMAT,
        "bundle_id": bundle.bundle_id,
        "slot_count": bundle.slots.len(),
        "functor_metadata": bundle.functor_metadata,
        "created_at": bundle.created_at,
        "slots": bundle.slots.iter().map(|s| json!({
            "concept": s.concept,
            "geometry_ref": s.geometry_ref,
            "crs": s.crs,
            "zedos_tag": s.zedos_tag,
            "preview": s.preview,
            "relation_count": s.relation_count,
        })).collect::<Vec<_>>(),
        "patches": bundle.patches.iter().map(|p| json!({
            "patch_id": p.patch_id,
            "morphism": p.morphism,
            "coeff_delta": p.coeff_delta,
        })).collect::<Vec<_>>(),
    })
}

pub fn bundle_provlog(bundle: &ContextBundle) -> String {
    let payload = serde_json::to_string_pretty(&bundle_to_json(bundle)).unwrap_or_default();
    format!(
        "# Context Variable Bundle\n\n**format:** {CONTEXT_BUNDLE_FORMAT}\n**var:** {}\n**slot_count:** {}\n**functor_metadata:** {}\n\n```json\n{payload}\n```\n",
        bundle.bundle_id,
        bundle.slots.len(),
        bundle.functor_metadata,
    )
}

pub fn parse_bundle_from_block(concept: &str, provlog: &str) -> Option<ContextBundle> {
    let start = provlog.find('{')?;
    let end = provlog.rfind('}')?;
    let v: Value = serde_json::from_str(&provlog[start..=end]).ok()?;
    if v.get("format").and_then(|x| x.as_str()) != Some(CONTEXT_BUNDLE_FORMAT) {
        return None;
    }
    let slots: Vec<ContextSlot> = v
        .get("slots")
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|s| {
                    Some(ContextSlot {
                        concept: s.get("concept")?.as_str()?.to_string(),
                        geometry_ref: s
                            .get("geometry_ref")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string(),
                        crs: s.get("crs").and_then(|x| x.as_f64()).unwrap_or(0.0) as f32,
                        zedos_tag: s.get("zedos_tag").and_then(|x| x.as_u64()).unwrap_or(0) as u8,
                        preview: s
                            .get("preview")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string(),
                        relation_count: s
                            .get("relation_count")
                            .and_then(|x| x.as_u64())
                            .unwrap_or(0) as usize,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let patches: Vec<LinguisticContextPatch> = v
        .get("patches")
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .map(|p| LinguisticContextPatch {
                    patch_id: p.get("patch_id").and_then(|x| x.as_u64()).unwrap_or(0) as u32,
                    morphism: p
                        .get("morphism")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string(),
                    coeff_delta: [0.0; 4],
                })
                .collect()
        })
        .unwrap_or_default();
    Some(ContextBundle {
        bundle_id: v
            .get("bundle_id")
            .and_then(|x| x.as_str())
            .unwrap_or(concept)
            .to_string(),
        slots,
        patches,
        functor_metadata: v
            .get("functor_metadata")
            .and_then(|x| x.as_str())
            .unwrap_or("context_var")
            .to_string(),
        created_at: v
            .get("created_at")
            .and_then(|x| x.as_u64())
            .unwrap_or(0),
    })
}

pub fn load_bundle(store: &StoreHandle, var_name: &str) -> Option<ContextBundle> {
    let key = normalize_var_name(var_name);
    if key.is_empty() {
        return None;
    }
    let block = store.fetch_block_high_priority(&key)?;
    let provlog = storage::read_provlog(&block);
    parse_bundle_from_block(&key, &provlog)
}

fn collect_concepts(
    store: &StoreHandle,
    concepts: &[String],
    prefixes: &[String],
    limit: usize,
) -> Vec<String> {
    let mut out = concepts
        .iter()
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty())
        .collect::<Vec<_>>();
    if !prefixes.is_empty() {
        let pref_refs: Vec<&str> = prefixes.iter().map(|s| s.as_str()).collect();
        for c in crate::scrub_export::candidates_by_prefix(
            store,
            &pref_refs,
            limit.saturating_sub(out.len()),
        ) {
            if !out.contains(&c) {
                out.push(c);
            }
        }
    }
    out.truncate(limit);
    out
}

pub struct DeclareResult {
    pub var_concept: String,
    pub bundle: ContextBundle,
    pub bound: usize,
    pub skipped: Vec<Value>,
}

pub fn var_declare(
    store: &mut StoreHandle,
    name: &str,
    concepts: &[String],
    prefixes: &[String],
    min_crs: f32,
    preview_chars: usize,
    functor_metadata: &str,
    limit: usize,
) -> Result<DeclareResult, String> {
    let var_concept = normalize_var_name(name);
    if var_concept.is_empty() {
        return Err("var name required".into());
    }
    let candidates = collect_concepts(store, concepts, prefixes, limit);
    if candidates.is_empty() {
        return Err("provide concepts and/or prefixes".into());
    }

    let mut slots = Vec::new();
    let mut skipped = Vec::new();
    for concept in candidates {
        if concept.starts_with("local:host:") || concept.starts_with("local:user:") {
            skipped.push(json!({ "concept": concept, "reason": "sovereignty_local_only" }));
            continue;
        }
        let Some(block) = store.fetch_block_high_priority(&concept) else {
            skipped.push(json!({ "concept": concept, "reason": "not_found" }));
            continue;
        };
        if block.crs_score < min_crs {
            skipped.push(json!({
                "concept": concept,
                "reason": "crs_below_min",
                "crs": block.crs_score,
            }));
            continue;
        }
        let provlog = storage::read_provlog(&block);
        let rel_n = store.search_relations(&concept, None, "both").len();
        slots.push(ContextSlot {
            concept: concept.clone(),
            geometry_ref: concept.clone(),
            crs: block.crs_score,
            zedos_tag: block.zedos_tag,
            preview: preview_text(&provlog, preview_chars),
            relation_count: rel_n,
        });
    }
    if slots.is_empty() {
        return Err("no slots bound after filters".into());
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let bundle = ContextBundle {
        bundle_id: var_concept.clone(),
        slots: slots.clone(),
        patches: Vec::new(),
        functor_metadata: functor_metadata.to_string(),
        created_at: now,
    };

    let mut block = store.encode(&bundle_provlog(&bundle));
    block.zedos_tag = ZEDOS_OPERATIONAL;
    block.crs_score = slots.iter().map(|s| s.crs).fold(0.0f32, f32::max).min(0.99);
    store
        .store(&var_concept, block)
        .map_err(|e| format!("store failed: {e}"))?;
    for slot in &slots {
        let _ = store.relate(&var_concept, "binds", &slot.concept);
    }

    Ok(DeclareResult {
        var_concept,
        bundle,
        bound: slots.len(),
        skipped,
    })
}

pub fn var_query(store: &StoreHandle, var_name: &str, mode: &str, preview_chars: usize) -> Result<Value, String> {
    let bundle = load_bundle(store, var_name).ok_or_else(|| format!("var not found: {var_name}"))?;
    let key = normalize_var_name(var_name);

    Ok(match mode {
        "preview" => {
            let slots: Vec<Value> = bundle
                .slots
                .iter()
                .map(|s| {
                    let full_preview = store
                        .fetch_block_high_priority(&s.concept)
                        .map(|b| preview_text(&storage::read_provlog(&b), preview_chars))
                        .unwrap_or_else(|| s.preview.clone());
                    json!({
                        "concept": s.concept,
                        "crs": s.crs,
                        "preview": full_preview,
                    })
                })
                .collect();
            json!({
                "var": key,
                "mode": "preview",
                "slot_count": slots.len(),
                "slots": slots,
            })
        }
        "relations" => {
            let mut edges = Vec::new();
            for slot in bundle.slots.iter().take(16) {
                for (label, other) in store.search_relations(&slot.concept, None, "both").into_iter().take(8) {
                    edges.push(json!({
                        "from": slot.concept,
                        "label": label,
                        "to": other,
                    }));
                }
            }
            json!({
                "var": key,
                "mode": "relations",
                "edge_count": edges.len(),
                "edges": edges,
            })
        }
        "slots" => json!({
            "var": key,
            "mode": "slots",
            "bundle": bundle_to_json(&bundle),
        }),
        _ => json!({
            "var": key,
            "mode": "metadata",
            "format": CONTEXT_BUNDLE_FORMAT,
            "slot_count": bundle.slots.len(),
            "functor_metadata": bundle.functor_metadata,
            "created_at": bundle.created_at,
            "slots": bundle.slots.iter().map(|s| json!({
                "concept": s.concept,
                "geometry_ref": s.geometry_ref,
                "crs": s.crs,
                "zedos_tag": s.zedos_tag,
                "relation_count": s.relation_count,
            })).collect::<Vec<_>>(),
        }),
    })
}

pub struct ProjectResult {
    pub var_concept: String,
    pub bundle: ContextBundle,
    pub operation: String,
}

pub fn var_project(
    store: &mut StoreHandle,
    source_var: &str,
    operation: &str,
    args: &Value,
    target_name: Option<&str>,
) -> Result<ProjectResult, String> {
    let source = load_bundle(store, source_var).ok_or_else(|| format!("source var not found: {source_var}"))?;

    let filtered: Vec<ContextSlot> = match operation {
        "filter_crs" => {
            let min = args.get("min_crs").and_then(|v| v.as_f64()).unwrap_or(0.74) as f32;
            source.slots.into_iter().filter(|s| s.crs >= min).collect()
        }
        "filter_prefix" => {
            let prefix = args.get("prefix").and_then(|v| v.as_str()).unwrap_or("");
            source
                .slots
                .into_iter()
                .filter(|s| s.concept.starts_with(prefix))
                .collect()
        }
        "merge_vars" => {
            let others: Vec<String> = args
                .get("vars")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(normalize_var_name))
                        .collect()
                })
                .unwrap_or_default();
            let mut merged = source.slots;
            let mut seen: HashSet<String> = merged.iter().map(|s| s.concept.clone()).collect();
            for ov in others {
                if let Some(b) = load_bundle(store, &ov) {
                    for s in b.slots {
                        if seen.insert(s.concept.clone()) {
                            merged.push(s);
                        }
                    }
                }
            }
            merged
        }
        "relate_neighborhood" => {
            let seed = args
                .get("seed")
                .and_then(|v| v.as_str())
                .or_else(|| source.slots.first().map(|s| s.concept.as_str()))
                .unwrap_or("");
            let k = args.get("k").and_then(|v| v.as_u64()).unwrap_or(8) as usize;
            let mut out = source.slots.clone();
            let mut seen: HashSet<String> = out.iter().map(|s| s.concept.clone()).collect();
            for (label, other) in store.search_relations(seed, None, "both").into_iter().take(k) {
                if !seen.insert(other.clone()) {
                    continue;
                }
                if let Some(block) = store.fetch_block_high_priority(&other) {
                    let provlog = storage::read_provlog(&block);
                    let rel_n = store.search_relations(&other, None, "both").len();
                    out.push(ContextSlot {
                        concept: other.clone(),
                        geometry_ref: other,
                        crs: block.crs_score,
                        zedos_tag: block.zedos_tag,
                        preview: preview_text(&provlog, 120),
                        relation_count: rel_n,
                    });
                    let _ = label;
                }
            }
            out
        }
        other => return Err(format!("unknown operation: {other}")),
    };

    if filtered.is_empty() {
        return Err("projection yielded zero slots".into());
    }

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let target = target_name
        .map(normalize_var_name)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("{}_{operation}_{ts}", source.bundle_id));

    let bundle = ContextBundle {
        bundle_id: target.clone(),
        slots: filtered,
        patches: source.patches,
        functor_metadata: format!("project:{operation}"),
        created_at: ts,
    };

    let mut block = store.encode(&bundle_provlog(&bundle));
    block.zedos_tag = ZEDOS_OPERATIONAL;
    block.crs_score = bundle.slots.iter().map(|s| s.crs).fold(0.0f32, f32::max).min(0.99);
    store
        .store(&target, block)
        .map_err(|e| format!("store failed: {e}"))?;
    let _ = store.relate(&source.bundle_id, "projects_to", &target);
    for slot in &bundle.slots {
        let _ = store.relate(&target, "binds", &slot.concept);
    }

    Ok(ProjectResult {
        var_concept: target,
        bundle,
        operation: operation.to_string(),
    })
}

pub struct RefreshProgramTracesResult {
    pub var_concept: String,
    pub bundle: ContextBundle,
    pub bound: usize,
    pub skipped: Vec<Value>,
}

/// Refresh `var:ctx_program_traces` with the last N trace concepts (max 8, preview 150 chars).
/// No-op when `trace_concepts` is empty or no slots bind.
pub fn refresh_program_traces_var(
    store: &mut StoreHandle,
    trace_concepts: &[String],
) -> Result<RefreshProgramTracesResult, String> {
    if trace_concepts.is_empty() {
        return Ok(RefreshProgramTracesResult {
            var_concept: VAR_CTX_PROGRAM_TRACES.to_string(),
            bundle: ContextBundle {
                bundle_id: VAR_CTX_PROGRAM_TRACES.to_string(),
                slots: Vec::new(),
                patches: Vec::new(),
                functor_metadata: "program_traces_session_end".to_string(),
                created_at: 0,
            },
            bound: 0,
            skipped: Vec::new(),
        });
    }

    let mut slots = Vec::new();
    let mut skipped = Vec::new();
    let mut seen = HashSet::new();

    for concept in trace_concepts {
        if slots.len() >= MAX_PROGRAM_TRACE_SLOTS {
            break;
        }
        let concept = concept.trim();
        if concept.is_empty() || !concept.starts_with("trace:") {
            continue;
        }
        if !seen.insert(concept.to_string()) {
            continue;
        }
        let Some(block) = store.fetch_block_high_priority(concept) else {
            skipped.push(json!({ "concept": concept, "reason": "not_found" }));
            continue;
        };
        let provlog = storage::read_provlog(&block);
        let rel_n = store.search_relations(concept, None, "both").len();
        slots.push(ContextSlot {
            concept: concept.to_string(),
            geometry_ref: concept.to_string(),
            crs: block.crs_score,
            zedos_tag: block.zedos_tag,
            preview: preview_text(&provlog, PROGRAM_TRACE_PREVIEW_CHARS),
            relation_count: rel_n,
        });
    }

    if slots.is_empty() {
        return Ok(RefreshProgramTracesResult {
            var_concept: VAR_CTX_PROGRAM_TRACES.to_string(),
            bundle: ContextBundle {
                bundle_id: VAR_CTX_PROGRAM_TRACES.to_string(),
                slots: Vec::new(),
                patches: Vec::new(),
                functor_metadata: "program_traces_session_end".to_string(),
                created_at: 0,
            },
            bound: 0,
            skipped,
        });
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let bundle = ContextBundle {
        bundle_id: VAR_CTX_PROGRAM_TRACES.to_string(),
        slots: slots.clone(),
        patches: Vec::new(),
        functor_metadata: "program_traces_session_end".to_string(),
        created_at: now,
    };

    let mut block = store.encode(&bundle_provlog(&bundle));
    block.zedos_tag = ZEDOS_OPERATIONAL;
    block.crs_score = slots.iter().map(|s| s.crs).fold(0.0f32, f32::max).min(0.99);
    store
        .store(VAR_CTX_PROGRAM_TRACES, block)
        .map_err(|e| format!("store failed: {e}"))?;
    for slot in &slots {
        let _ = store.relate(VAR_CTX_PROGRAM_TRACES, "binds", &slot.concept);
    }

    Ok(RefreshProgramTracesResult {
        var_concept: VAR_CTX_PROGRAM_TRACES.to_string(),
        bundle,
        bound: slots.len(),
        skipped,
    })
}

/// Bridge ContextBundle → LinguisticDiscourseBundle for mcp_linguistic_calculus.
pub fn context_bundle_to_linguistic(bundle: &ContextBundle) -> LinguisticDiscourseBundle {
    LinguisticDiscourseBundle {
        bundle_id: bundle.bundle_id.clone(),
        words: bundle
            .slots
            .iter()
            .map(|s| LinguisticWord {
                text: s.concept.clone(),
                coeff: {
                    let mut c = [0.0f32; 8];
                    c[0] = s.crs;
                    c[1] = s.relation_count as f32 * 0.01;
                    c
                },
            })
            .collect(),
        patches: bundle.patches.clone(),
        functor_metadata: format!("{}_linguistic", bundle.functor_metadata),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_var_prefix() {
        assert_eq!(normalize_var_name("ctx"), "var:ctx");
        assert_eq!(normalize_var_name("var:ctx"), "var:ctx");
    }

    #[test]
    fn declare_and_query_roundtrip() {
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        std::env::set_var("ENGRAM_FORCE_CPU_BACKEND", "1");
        std::env::set_var("ENGRAM_KI_DISABLE", "1");
        let dir = std::env::temp_dir().join(format!(
            "ctx_var_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        ));
        std::fs::create_dir_all(&dir).ok();
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store.remember("trace:var_test_a", "**decision:** test A").unwrap();
        store.remember("trace:var_test_b", "**decision:** test B").unwrap();
        let r = var_declare(
            &mut store,
            "test_bundle",
            &[
                "trace:var_test_a".into(),
                "trace:var_test_b".into(),
            ],
            &[],
            0.5,
            40,
            "test",
            8,
        )
        .expect("declare");
        assert_eq!(r.bound, 2);
        let q = var_query(&store, &r.var_concept, "metadata", 40).unwrap();
        assert_eq!(q.get("slot_count").and_then(|v| v.as_u64()), Some(2));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn refresh_program_traces_var_binds_trace_slots() {
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        std::env::set_var("ENGRAM_FORCE_CPU_BACKEND", "1");
        std::env::set_var("ENGRAM_KI_DISABLE", "1");
        let dir = std::env::temp_dir().join(format!(
            "ctx_refresh_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        ));
        std::fs::create_dir_all(&dir).ok();
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember("trace:prog_a", "**decision:** alpha decision with enough text")
            .unwrap();
        store
            .remember("trace:prog_b", "**decision:** beta decision with enough text")
            .unwrap();

        let r = refresh_program_traces_var(
            &mut store,
            &["trace:prog_a".into(), "trace:prog_b".into()],
        )
        .expect("refresh");

        assert_eq!(r.var_concept, VAR_CTX_PROGRAM_TRACES);
        assert_eq!(r.bound, 2);
        let q = var_query(&store, VAR_CTX_PROGRAM_TRACES, "metadata", 150).unwrap();
        assert_eq!(q.get("slot_count").and_then(|v| v.as_u64()), Some(2));
        assert_eq!(
            q.get("functor_metadata").and_then(|v| v.as_str()),
            Some("program_traces_session_end")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn refresh_program_traces_var_caps_at_eight_and_preview() {
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        std::env::set_var("ENGRAM_FORCE_CPU_BACKEND", "1");
        std::env::set_var("ENGRAM_KI_DISABLE", "1");
        let dir = std::env::temp_dir().join(format!(
            "ctx_refresh_cap_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        ));
        std::fs::create_dir_all(&dir).ok();
        let mut store = StoreHandle::new(&dir.to_string_lossy());

        let long_body = "x".repeat(200);
        let mut concepts = Vec::new();
        for i in 0..10 {
            let key = format!("trace:cap_{i}");
            store.remember(&key, &long_body).unwrap();
            concepts.push(key);
        }

        let r = refresh_program_traces_var(&mut store, &concepts).expect("refresh");
        assert_eq!(r.bound, MAX_PROGRAM_TRACE_SLOTS);

        let bundle = load_bundle(&store, VAR_CTX_PROGRAM_TRACES).expect("bundle");
        assert_eq!(bundle.slots.len(), MAX_PROGRAM_TRACE_SLOTS);
        for slot in &bundle.slots {
            assert!(slot.preview.chars().count() <= PROGRAM_TRACE_PREVIEW_CHARS + 1);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn refresh_program_traces_var_empty_is_noop() {
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        std::env::set_var("ENGRAM_FORCE_CPU_BACKEND", "1");
        std::env::set_var("ENGRAM_KI_DISABLE", "1");
        let dir = std::env::temp_dir().join(format!(
            "ctx_refresh_empty_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        ));
        std::fs::create_dir_all(&dir).ok();
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        let r = refresh_program_traces_var(&mut store, &[]).expect("refresh");
        assert_eq!(r.bound, 0);
        assert!(load_bundle(&store, VAR_CTX_PROGRAM_TRACES).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}