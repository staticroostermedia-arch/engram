//! E3 — Counterfactual / branch memory (session-scoped active branch + concept tags).

use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct BranchRecord {
    pub id: String,
    pub label: String,
    pub parent: String,
    pub status: String, // open|merged|abandoned
    pub created_at: u64,
}

/// Process-local branch registry (tests + single MCP process). Durable blocks go via store.
static BRANCHES: Mutex<Option<HashMap<String, BranchRecord>>> = Mutex::new(None);
static ACTIVE: Mutex<Option<String>> = Mutex::new(None);
/// Concepts written under a branch (concept -> branch_id)
static BRANCH_TAGS: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn with_map<R>(f: impl FnOnce(&mut HashMap<String, BranchRecord>) -> R) -> R {
    let mut g = BRANCHES.lock().unwrap();
    if g.is_none() {
        *g = Some(HashMap::new());
    }
    f(g.as_mut().unwrap())
}

fn with_tags<R>(f: impl FnOnce(&mut HashMap<String, String>) -> R) -> R {
    let mut g = BRANCH_TAGS.lock().unwrap();
    if g.is_none() {
        *g = Some(HashMap::new());
    }
    f(g.as_mut().unwrap())
}

pub fn branch_create(from: &str, label: &str) -> Value {
    let id = format!(
        "branch:{}-{}",
        now(),
        label
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .take(32)
            .collect::<String>()
    );
    let rec = BranchRecord {
        id: id.clone(),
        label: label.to_string(),
        parent: from.to_string(),
        status: "open".into(),
        created_at: now(),
    };
    with_map(|m| {
        m.insert(id.clone(), rec.clone());
    });
    json!({
        "version": "branch_memory_v1",
        "branch": {
            "id": rec.id,
            "label": rec.label,
            "parent": rec.parent,
            "status": rec.status,
            "created_at": rec.created_at,
        },
        "receipt": format!("receipt:branch_create_{}", rec.id),
    })
}

pub fn branch_checkout(branch_id: &str) -> Value {
    if branch_id == "main" || branch_id.is_empty() {
        *ACTIVE.lock().unwrap() = None;
        return json!({
            "version": "branch_memory_v1",
            "active_branch": null,
            "mainline": true,
        });
    }
    let exists = with_map(|m| m.contains_key(branch_id));
    if !exists {
        return json!({
            "error": "branch_not_found",
            "branch_id": branch_id,
        });
    }
    *ACTIVE.lock().unwrap() = Some(branch_id.to_string());
    json!({
        "version": "branch_memory_v1",
        "active_branch": branch_id,
        "mainline": false,
    })
}

pub fn active_branch() -> Option<String> {
    ACTIVE.lock().unwrap().clone()
}

pub fn tag_write(concept: &str) {
    if let Some(b) = active_branch() {
        with_tags(|t| {
            t.insert(concept.to_string(), b);
        });
    }
}

#[allow(dead_code)]
pub fn concept_branch(concept: &str) -> Option<String> {
    with_tags(|t| t.get(concept).cloned())
}

/// Mainline anchors omit branch-only concepts.
pub fn filter_mainline_anchors(concepts: &[String]) -> Vec<String> {
    let active = active_branch();
    with_tags(|tags| {
        concepts
            .iter()
            .filter(|c| match tags.get(c.as_str()) {
                None => true,
                Some(b) => {
                    // Visible if checked out on that branch
                    active.as_ref() == Some(b)
                }
            })
            .cloned()
            .collect()
    })
}

pub fn branch_merge(branch_id: &str, strategy: &str) -> Value {
    let mut ok = false;
    with_map(|m| {
        if let Some(b) = m.get_mut(branch_id) {
            if b.status == "open" {
                b.status = "merged".into();
                ok = true;
            }
        }
    });
    if !ok {
        return json!({"error": "merge_failed", "branch_id": branch_id});
    }
    // On merge, clear tags so concepts become mainline-visible
    if strategy != "prefer_main" {
        with_tags(|t| {
            t.retain(|_, b| b != branch_id);
        });
    }
    if active_branch().as_deref() == Some(branch_id) {
        *ACTIVE.lock().unwrap() = None;
    }
    json!({
        "version": "branch_memory_v1",
        "merged": branch_id,
        "strategy": strategy,
        "receipt": format!("receipt:branch_merge_{branch_id}"),
        "status": "merged",
    })
}

pub fn branch_abandon(branch_id: &str) -> Value {
    let mut ok = false;
    with_map(|m| {
        if let Some(b) = m.get_mut(branch_id) {
            b.status = "abandoned".into();
            ok = true;
        }
    });
    if !ok {
        return json!({"error": "abandon_failed", "branch_id": branch_id});
    }
    if active_branch().as_deref() == Some(branch_id) {
        *ACTIVE.lock().unwrap() = None;
    }
    json!({
        "version": "branch_memory_v1",
        "abandoned": branch_id,
        "scar": format!("scar:branch_abandoned_{branch_id}"),
        "status": "abandoned",
    })
}

/// Test helper: reset process-local state.
#[cfg(test)]
pub fn reset_for_tests() {
    *BRANCHES.lock().unwrap() = Some(HashMap::new());
    *ACTIVE.lock().unwrap() = None;
    *BRANCH_TAGS.lock().unwrap() = Some(HashMap::new());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isolation_and_merge() {
        let label = format!("explore-{}", std::process::id());
        let tile = format!("tile:branch_only_{}", std::process::id());
        let c = branch_create("trace:root", &label);
        let id = c["branch"]["id"].as_str().unwrap().to_string();
        branch_checkout(&id);
        tag_write(&tile);
        // mainline filter hides branch concept when checkout main
        branch_checkout("main");
        let anchors = filter_mainline_anchors(&["goal:main".into(), tile.clone()]);
        assert!(anchors.contains(&"goal:main".into()));
        assert!(!anchors.contains(&tile));
        let m = branch_merge(&id, "prefer_branch");
        assert_eq!(m["status"], "merged");
        assert!(m.get("receipt").is_some());
    }

    #[test]
    fn abandon_scars() {
        let label = format!("dead-{}", std::process::id());
        let c = branch_create("goal:x", &label);
        let id = c["branch"]["id"].as_str().unwrap().to_string();
        let a = branch_abandon(&id);
        assert_eq!(a["status"], "abandoned", "{a}");
        assert!(a["scar"].as_str().unwrap().starts_with("scar:"));
    }
}
