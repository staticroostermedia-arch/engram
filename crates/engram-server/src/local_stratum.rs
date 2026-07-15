//! Local Context Stratum (LCS) — always-hot host/project blocks on NVMe, surfaced at wake.
//!
//! Extends effective context window onto `.leg` files without token explosion.
//! Sovereignty: `local:host:*` and `local:project:*` default to `local_only` / export deny.

use crate::store::StoreHandle;
use engram_core::storage;
use engram_core::types::ZEDOS_DECLARATIVE;
use serde_json::{json, Value};

pub const LOCAL_HOST_PROFILE: &str = "local:host:profile";
pub const LOCAL_HOST_MCP: &str = "local:host:mcp";
pub const LOCAL_HOST_READINESS: &str = "local:host:readiness_cache";

const PROFILE_TTL_SECS: u64 = 86_400;
const READINESS_TTL_SECS: u64 = 300;

pub fn enabled() -> bool {
    !matches!(
        std::env::var("ENGRAM_LOCAL_STRATUM")
            .unwrap_or_else(|_| "1".to_string())
            .to_ascii_lowercase()
            .as_str(),
        "0" | "false" | "off"
    )
}

pub fn local_budget() -> usize {
    if let Ok(v) = std::env::var("ENGRAM_LOCAL_STRATUM_K") {
        if let Ok(n) = v.parse::<usize>() {
            return n.clamp(4, 32);
        }
    }
    12
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn nvidia_gpu_count() -> u32 {
    // RSI Cycle 52: process-lifetime cache — nvidia-smi was multi-ms per wake bootstrap.
    static CACHED: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *CACHED.get_or_init(|| {
        std::process::Command::new("nvidia-smi")
            .args(["--query-gpu=index", "--format=csv,noheader"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .count() as u32
            })
            .unwrap_or(0)
    })
}

fn git_project_fingerprint() -> Option<(String, String, String)> {
    // RSI Cycle 52: cache git root/branch for process lifetime (2× subprocess per call).
    static CACHED: std::sync::OnceLock<Option<(String, String, String)>> =
        std::sync::OnceLock::new();
    CACHED
        .get_or_init(|| {
            let root = std::process::Command::new("git")
                .args(["rev-parse", "--show-toplevel"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .filter(|s| !s.is_empty())?;

            let branch = std::process::Command::new("git")
                .args(["-C", &root, "rev-parse", "--abbrev-ref", "HEAD"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_else(|| "unknown".to_string());

            let fp = blake3::hash(root.as_bytes());
            let short = &fp.to_hex()[..8];
            let concept = format!("local:project:{short}:root");
            Some((concept, root, branch))
        })
        .clone()
}

fn effective_store_path() -> String {
    std::env::var("ENGRAM_STORE")
        .map(|s| shellexpand::tilde(&s).into_owned())
        .unwrap_or_else(|_| "~/.engram/stalks/".to_string())
}

fn host_profile_text(store: &StoreHandle) -> String {
    let profile = crate::profile::current_profile_name();
    let memory_mode = StoreHandle::memory_mode();
    let store_path = effective_store_path();
    let readiness = store.backend_readiness();
    let leg_count = readiness
        .get("leg_block_count")
        .and_then(|v| v.as_u64())
        .unwrap_or_else(|| store.leg_block_count() as u64);
    let gpu_n = nvidia_gpu_count();
    let mut lines = vec![
        "# Local Host Profile".to_string(),
        String::new(),
        "**sovereignty:** local_only".to_string(),
        "**export_policy:** deny".to_string(),
        String::new(),
        format!(
            "**os:** {} {}",
            std::env::consts::OS,
            std::env::consts::ARCH
        ),
        format!("**engram_profile:** {profile}"),
        format!("**memory_mode:** {memory_mode}"),
        format!("**store_path:** {store_path}"),
        format!("**leg_block_count:** {leg_count}"),
        format!("**gpu_count:** {gpu_n}"),
        format!("**refreshed_at:** {}", now_secs()),
    ];
    if let Some((concept, root, branch)) = git_project_fingerprint() {
        lines.push(format!("**active_project:** {concept}"));
        lines.push(format!("**git_root:** {root}"));
        lines.push(format!("**git_branch:** {branch}"));
    }
    lines.join("\n")
}

fn host_mcp_text() -> String {
    let store = effective_store_path();
    let profile = std::env::var("ENGRAM_PROFILE").unwrap_or_else(|_| "agent".to_string());
    let llm_url = std::env::var("ENGRAM_LLM_URL")
        .or_else(|_| std::env::var("ENGRAM_SCOUT_LLM_URL"))
        .unwrap_or_else(|_| "(unset)".to_string());
    let turn_llm = std::env::var("ENGRAM_TURN_LLM_EXTRACT").unwrap_or_else(|_| "0".to_string());
    format!(
        "# Local MCP Wiring\n\n**sovereignty:** local_only\n**export_policy:** deny\n\n**engram_profile:** {profile}\n**engram_store:** {store}\n**wake_bundle:** {}\n**local_stratum:** enabled={}\n**ENGRAM_LLM_URL:** {llm_url}\n**ENGRAM_TURN_LLM_EXTRACT:** {turn_llm}\n",
        std::env::var("ENGRAM_WAKE_BUNDLE").unwrap_or_else(|_| "slim".to_string()),
        enabled()
    )
}

fn block_text(store: &StoreHandle, concept: &str) -> Option<String> {
    store
        .fetch_block(concept)
        .map(|b| storage::read_provlog(&b))
}

fn is_corrupted_local_block(text: &str) -> bool {
    text.contains("engram_lcs_test") || text.contains("/tmp/engram_lcs_test")
}

fn upsert_declarative(store: &mut StoreHandle, concept: &str, text: &str) -> bool {
    if let Some(existing) = block_text(store, concept) {
        if is_corrupted_local_block(&existing) {
            return store.remember(concept, text).is_ok();
        }
        return store.update(concept, text).is_ok();
    }
    store.remember(concept, text).is_ok()
}

fn touch_block_crs(store: &mut StoreHandle, concept: &str) {
    if let Some(mut block) = store.fetch_block(concept) {
        block.zedos_tag = ZEDOS_DECLARATIVE;
        block.crs_score = 1.0;
        let _ = store.store(concept, block);
    }
}

fn stale(concept: &str, store: &StoreHandle, ttl: u64) -> bool {
    let ts = store.access_index.last_accessed(concept).unwrap_or(0);
    if ts == 0 {
        return true;
    }
    now_secs().saturating_sub(ts) > ttl
}

/// RSI Cycle 52/54: true when wake can skip full bootstrap.
/// Cycle 52 required is_hot + fresh readiness — still paid ~3s when readiness
/// soft-stale or hot_set lag. Cycle 54: profile block present is enough for wake.
pub fn warm_skip_bootstrap(store: &StoreHandle) -> bool {
    if !enabled() {
        return true;
    }
    store
        .fetch_block_high_priority(LOCAL_HOST_PROFILE)
        .is_some()
        || store.fetch_block(LOCAL_HOST_PROFILE).is_some()
}

/// Wake-path bootstrap: skip expensive refresh when local host profile already exists.
/// Full `bootstrap` still runs on cold first mint and on non-wake paths.
pub fn bootstrap_for_wake(store: &mut StoreHandle) -> Vec<String> {
    if warm_skip_bootstrap(store) {
        return vec![];
    }
    bootstrap(store)
}

/// Bootstrap or refresh local layer blocks; promote to hot path.
pub fn bootstrap(store: &mut StoreHandle) -> Vec<String> {
    if !enabled() {
        return vec![];
    }

    let mut touched = Vec::new();

    let profile_txt = host_profile_text(store);
    let profile_stale = stale(LOCAL_HOST_PROFILE, store, PROFILE_TTL_SECS)
        || block_text(store, LOCAL_HOST_PROFILE).is_none()
        || block_text(store, LOCAL_HOST_PROFILE)
            .map(|t| is_corrupted_local_block(&t) || !t.contains(&effective_store_path()))
            .unwrap_or(true);
    if profile_stale && upsert_declarative(store, LOCAL_HOST_PROFILE, &profile_txt) {
        touch_block_crs(store, LOCAL_HOST_PROFILE);
        store.mark_hot(LOCAL_HOST_PROFILE);
        touched.push(LOCAL_HOST_PROFILE.to_string());
    }

    let mcp_txt = host_mcp_text();
    if (store.fetch_block(LOCAL_HOST_MCP).is_none()
        || stale(LOCAL_HOST_MCP, store, PROFILE_TTL_SECS))
        && upsert_declarative(store, LOCAL_HOST_MCP, &mcp_txt)
    {
        touch_block_crs(store, LOCAL_HOST_MCP);
        store.mark_hot(LOCAL_HOST_MCP);
        touched.push(LOCAL_HOST_MCP.to_string());
    }

    if let Some((concept, root, branch)) = git_project_fingerprint() {
        let text = format!(
            "# Local Project Root\n\n**sovereignty:** project_local\n**export_policy:** scrub_required\n\n**git_root:** {root}\n**git_branch:** {branch}\n**refreshed_at:** {}\n",
            now_secs()
        );
        if (store.fetch_block(&concept).is_none() || stale(&concept, store, PROFILE_TTL_SECS))
            && upsert_declarative(store, &concept, &text)
        {
            touch_block_crs(store, &concept);
            store.mark_hot(&concept);
            touched.push(concept);
        }
    }

    let readiness = store.backend_readiness();
    let readiness_txt = format!(
        "# Local Readiness Cache\n\n**sovereignty:** local_only\n**export_policy:** deny\n\n```json\n{}\n```\n",
        serde_json::to_string_pretty(&readiness).unwrap_or_else(|_| "{}".to_string())
    );
    if (stale(LOCAL_HOST_READINESS, store, READINESS_TTL_SECS)
        || store.fetch_block(LOCAL_HOST_READINESS).is_none())
        && upsert_declarative(store, LOCAL_HOST_READINESS, &readiness_txt)
    {
        touch_block_crs(store, LOCAL_HOST_READINESS);
        store.mark_hot(LOCAL_HOST_READINESS);
        touched.push(LOCAL_HOST_READINESS.to_string());
    }

    let _ = store.relate(
        "process:engram.ritual.local-context-working-memory",
        "produces",
        LOCAL_HOST_PROFILE,
    );
    let _ = store.relate(
        "ritual:engram.working-memory",
        "enforced_by",
        LOCAL_HOST_PROFILE,
    );

    touched
}

fn preview_for(store: &StoreHandle, concept: &str) -> Option<Value> {
    let block = store.fetch_block_high_priority(concept)?;
    let text = storage::read_provlog(&block);
    let preview: String = text
        .lines()
        .skip_while(|l| l.starts_with('#'))
        .take(4)
        .collect::<Vec<_>>()
        .join(" ");
    let preview = if preview.len() > 160 {
        format!("{}…", preview.chars().take(159).collect::<String>())
    } else if preview.is_empty() {
        text.chars().take(120).collect()
    } else {
        preview
    };
    let tier = if concept.starts_with("local:host:") {
        "local_only"
    } else if concept.starts_with("local:project:") {
        "project_local"
    } else {
        "local"
    };
    Some(json!({
        "concept": concept,
        "preview": preview,
        "crs": block.crs_score,
        "hot": store.is_hot(concept),
        "tier": tier,
    }))
}

/// Lean wake slice — previews only, bounded by budget.
pub fn build_local_stratum_slice(store: &StoreHandle, budget: usize) -> Value {
    if !enabled() {
        return json!({
            "version": "v1",
            "enabled": false,
            "node_count": 0,
            "nodes": [],
            "sovereignty_note": "ENGRAM_LOCAL_STRATUM=off",
        });
    }

    let mut concepts: Vec<String> = vec![
        LOCAL_HOST_PROFILE.to_string(),
        LOCAL_HOST_MCP.to_string(),
        LOCAL_HOST_READINESS.to_string(),
    ];
    if let Some((c, _, _)) = git_project_fingerprint() {
        concepts.push(c);
    }
    // Cycle 52: only scan recent when budget still open beyond core four (wake budget often 4–12).
    if budget > concepts.len() {
        for (c, _) in store.access_index.recent(16) {
            if c.starts_with("local:") && !concepts.contains(&c) {
                concepts.push(c);
            }
            if concepts.len() >= budget {
                break;
            }
        }
    }

    let mut nodes: Vec<Value> = Vec::new();
    for c in concepts {
        if nodes.len() >= budget {
            break;
        }
        if let Some(n) = preview_for(store, &c) {
            nodes.push(n);
        }
    }

    json!({
        "version": "v1",
        "enabled": true,
        "budget": budget,
        "node_count": nodes.len(),
        "sovereignty_note": "local_only blocks never export raw — use scrub_export when implemented",
        "process": "process:engram.ritual.local-context-working-memory",
        "nodes": nodes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::StoreHandle;

    #[test]
    fn bootstrap_mints_host_profile() {
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        let dir = std::env::temp_dir().join(format!("engram_lcs_test_{}", now_secs()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.to_string_lossy().to_string();
        let mut lock = StoreHandle::new(&path);
        let touched = bootstrap(&mut lock);
        assert!(
            touched.iter().any(|c| c == LOCAL_HOST_PROFILE),
            "bootstrap should touch local:host:profile"
        );
        let slice = build_local_stratum_slice(&lock, 8);
        assert_eq!(slice["enabled"], true);
        assert!(slice["node_count"].as_u64().unwrap_or(0) >= 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RSI Cycle 52/54: second wake skip when host profile block exists.
    #[test]
    fn warm_skip_bootstrap_after_first_bootstrap() {
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        std::env::set_var("ENGRAM_FORCE_CPU_BACKEND", "1");
        let dir = std::env::temp_dir().join(format!("engram_lcs_warm_skip_{}", now_secs()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.to_string_lossy().to_string();
        let mut lock = StoreHandle::new(&path);
        assert!(
            !warm_skip_bootstrap(&lock),
            "cold store must not skip bootstrap"
        );
        let _ = bootstrap(&mut lock);
        assert!(
            warm_skip_bootstrap(&lock),
            "after bootstrap, profile exists → warm_skip"
        );
        let second = bootstrap_for_wake(&mut lock);
        assert!(
            second.is_empty(),
            "bootstrap_for_wake must no-op when profile present: got {second:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
