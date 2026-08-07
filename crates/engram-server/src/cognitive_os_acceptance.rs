//! Fixed acceptance suite for cognitive OS plan gates (product entry points only).
//!
//! Filter: `cognitive_os_acceptance`
//! Capture: `cargo test -p engram-server --bin engram cognitive_os_acceptance -- --test-threads=1`

#[cfg(test)]
mod tests {
    use crate::mcp::handle_tool_call;
    use crate::store::StoreHandle;
    use serde_json::json;
    use std::sync::{Arc, Mutex};

    type SharedStore = Arc<Mutex<StoreHandle>>;

    fn handle_tool_on_big_stack(
        name: &str,
        args: &serde_json::Value,
        store: &SharedStore,
    ) -> serde_json::Value {
        let name = name.to_string();
        let args = args.clone();
        let store = Arc::clone(store);
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(move || handle_tool_call(&name, &args, &store))
            .expect("spawn")
            .join()
            .expect("join")
    }

    fn unique_tmp(prefix: &str) -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = format!("/tmp/engram-coa-{prefix}-{}-{}", std::process::id(), nanos);
        std::fs::create_dir_all(&dir).ok();
        dir
    }

    fn prep_store(tmp: &str) -> SharedStore {
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        std::env::set_var("ENGRAM_FORCE_CPU_BACKEND", "1");
        std::env::set_var("ENGRAM_KI_DISABLE", "1");
        let store = Arc::new(Mutex::new(StoreHandle::new(tmp)));
        store.lock().unwrap().mark_fully_initialized();
        store
    }

    fn is_error(resp: &serde_json::Value) -> bool {
        resp.get("isError")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    fn text_of(resp: &serde_json::Value) -> String {
        resp.pointer("/content/0/text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    }

    /// E1 smoke: pure budget path (already shipped).
    #[test]
    fn e1_budget_anchors_before_noise() {
        let slim = json!({
            "primary_goal": "goal:x",
            "cold_start_fidelity": {"score": 0.9},
            "presentation_stratum": {"previews": ["a","b","c","d","e","f"]},
            "relation_resume": {"edges": [1,2,3,4,5]},
            "bundle_tier": "slim",
            "recall_hint": "h",
        });
        let (pkt, meta) = crate::wake_budget::apply_wake_budget(&slim, Some(40), "anchors_first");
        assert_eq!(meta["truncated"], true);
        assert!(pkt.get("primary_goal").is_some());
        let omitted = meta["omitted_slots"].as_array().unwrap();
        assert!(
            omitted.iter().any(|s| {
                matches!(
                    s.as_str(),
                    Some("presentation_stratum") | Some("relation_resume")
                )
            }),
            "noise omitted: {omitted:?}"
        );
        assert!(
            crate::wake_budget::default_max_tokens_for_profile("minimal")
                < crate::wake_budget::default_max_tokens_for_profile("cuda_dual")
        );
    }

    /// E3: store(trace) + handle_tool_call(quick_trace) branch-tagged; main anchors omit.
    #[test]
    fn e3_branch_store_and_quick_trace_isolated() {
        let _branch = crate::branch_memory::env_test_lock();
        crate::branch_memory::reset_for_tests();
        let _consult = crate::consult_before_write_gate::env_test_lock();
        std::env::set_var("ENGRAM_CONSULT_BEFORE_WRITE", "off");
        std::env::set_var("ENGRAM_TOOL_TIER", "power");
        let tmp = unique_tmp("e3");
        let store = prep_store(&tmp);

        let created = handle_tool_on_big_stack(
            "mcp_engram_branch_create",
            &json!({"from_goal": "goal:root", "label": "coa_e3"}),
            &store,
        );
        assert!(!is_error(&created), "{created}");
        let raw = text_of(&created);
        let json_part = raw.split("\n\n").next().unwrap_or(&raw);
        let cj: serde_json::Value = serde_json::from_str(json_part).unwrap_or(json!({}));
        let branch_id = cj
            .pointer("/branch/id")
            .and_then(|v| v.as_str())
            .unwrap()
            .to_string();

        handle_tool_on_big_stack(
            "mcp_engram_branch_checkout",
            &json!({"branch_id": branch_id}),
            &store,
        );

        // Direct store path (trace)
        let concept = format!("trace:coa_store_{}", std::process::id());
        {
            let mut s = store.lock().unwrap();
            let mut blk = s.encode("TRACE\n\n**decision:** coa store\n");
            blk.crs_score = 0.9;
            s.store(&concept, blk).unwrap();
        }
        assert!(crate::branch_memory::concept_branch(&concept).is_some());

        // quick_trace path
        let before: std::collections::HashSet<_> = store
            .lock()
            .unwrap()
            .list()
            .into_iter()
            .filter(|c| c.starts_with("trace:"))
            .collect();
        let tr = handle_tool_on_big_stack(
            "mcp_engram_quick_trace",
            &json!({
                "decision": "coa quick_trace branch isolation",
                "why": "acceptance matrix E3"
            }),
            &store,
        );
        assert!(!is_error(&tr), "{tr}");
        let new_traces: Vec<_> = store
            .lock()
            .unwrap()
            .list()
            .into_iter()
            .filter(|c| c.starts_with("trace:") && !before.contains(c))
            .collect();
        assert!(!new_traces.is_empty(), "quick_trace minted trace");
        let qt = new_traces[0].clone();
        assert!(crate::branch_memory::concept_branch(&qt).is_some());

        handle_tool_on_big_stack(
            "mcp_engram_branch_checkout",
            &json!({"branch_id": "main"}),
            &store,
        );
        assert!(!StoreHandle::concept_visible_in_anchors(&concept));
        assert!(!StoreHandle::concept_visible_in_anchors(&qt));
        let (hits, _) = store
            .lock()
            .unwrap()
            .recall_scoped(&concept, 5, Some("anchors"));
        assert!(hits.iter().all(|m| m.concept != concept && m.concept != qt));

        handle_tool_on_big_stack(
            "mcp_engram_branch_abandon",
            &json!({"branch_id": branch_id}),
            &store,
        );
        std::env::remove_var("ENGRAM_CONSULT_BEFORE_WRITE");
        std::env::remove_var("ENGRAM_TOOL_TIER");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// E5: handle_tool_call remember under foreign lease → isError + no block + conflict.
    #[test]
    fn e5_lease_blocks_remember_handle_tool() {
        let _consult = crate::consult_before_write_gate::env_test_lock();
        let _lease = crate::lease_conflict::env_test_lock();
        std::env::set_var("ENGRAM_CONSULT_BEFORE_WRITE", "off");
        std::env::set_var("ENGRAM_LEASE_ENFORCE", "1");
        std::env::set_var("ENGRAM_AGENT_ID", "writer_b");
        let tmp = unique_tmp("e5");
        let store = prep_store(&tmp);
        let concept = format!("goal:coa_lease_{}", std::process::id());
        assert_eq!(
            crate::lease_conflict::lease_acquire(&concept, "writer_a", 60_000)["ok"],
            true
        );

        let resp = handle_tool_on_big_stack(
            "mcp_engram_remember",
            &json!({"concept": concept, "text": "blocked by lease"}),
            &store,
        );
        assert!(is_error(&resp), "must isError: {resp}");
        assert!(text_of(&resp).contains("lease"));
        assert!(store.lock().unwrap().fetch_block(&concept).is_none());
        assert!(store
            .lock()
            .unwrap()
            .list()
            .iter()
            .any(|c| c.starts_with("conflict:")));

        // choke is ensure_user_write_allowed (not duplicated handler logic)
        assert!(store
            .lock()
            .unwrap()
            .ensure_user_write_allowed(&concept)
            .is_err());

        crate::lease_conflict::lease_break(&concept);
        std::env::remove_var("ENGRAM_LEASE_ENFORCE");
        std::env::remove_var("ENGRAM_AGENT_ID");
        std::env::remove_var("ENGRAM_CONSULT_BEFORE_WRITE");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// E9: foreign omitted from anchors until accept.
    #[test]
    fn e9_foreign_anchors_until_accept() {
        let tmp = unique_tmp("e9");
        let mut store = StoreHandle::new(&tmp);
        let (concept, body, crs) =
            crate::foreign_stratum::build_foreign_payload("docs", "foreign body", "a.md");
        let mut blk = store.encode(&body);
        blk.crs_score = crs;
        store.store(&concept, blk).unwrap();
        crate::foreign_stratum::register_foreign(&concept);
        assert!(!StoreHandle::concept_visible_in_anchors(&concept));
        let (hits, _) = store.recall_scoped(&concept, 5, Some("anchors"));
        assert!(hits.iter().all(|m| m.concept != concept));
        assert_eq!(
            crate::foreign_stratum::accept_external(&concept)["ok"],
            true
        );
        assert!(StoreHandle::concept_visible_in_anchors(&concept));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
