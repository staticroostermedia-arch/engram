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

/// RSI Cycle 82: soft-stale wake local_stratum Value (default 1800s, sliding).
struct LocalWakeSliceCache {
    store_key: String,
    last_ok: Option<std::time::Instant>,
    value: Option<Value>,
}

static LOCAL_WAKE_SLICE_CACHE: std::sync::LazyLock<std::sync::Mutex<LocalWakeSliceCache>> =
    std::sync::LazyLock::new(|| {
        std::sync::Mutex::new(LocalWakeSliceCache {
            store_key: String::new(),
            last_ok: None,
            value: None,
        })
    });

fn local_wake_soft_stale_secs() -> u64 {
    std::env::var("ENGRAM_LOCAL_WAKE_SOFT_STALE_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1800)
}

fn local_wake_slice_cache_get(store_key: &str) -> Option<Value> {
    let soft = local_wake_soft_stale_secs();
    if soft == 0 || store_key.is_empty() {
        return None;
    }
    let mut cache = LOCAL_WAKE_SLICE_CACHE.lock().ok()?;
    if cache.store_key != store_key {
        return None;
    }
    let t = cache.last_ok?;
    if t.elapsed().as_secs() >= soft {
        return None;
    }
    // Sliding window (same pattern as sheaf C81).
    cache.last_ok = Some(std::time::Instant::now());
    cache.value.clone()
}

fn local_wake_slice_cache_set(store_key: &str, value: Value) {
    if store_key.is_empty() {
        return;
    }
    if let Ok(mut cache) = LOCAL_WAKE_SLICE_CACHE.lock() {
        cache.store_key = store_key.to_string();
        cache.last_ok = Some(std::time::Instant::now());
        cache.value = Some(value);
    }
}

/// RSI Cycle 62: ultra-lean wake local stratum.
/// Skip readiness_cache (multi-KB JSON; full readiness already on session_start packet),
/// skip recent local: walk and project git concept when not needed for sovereignty hint.
/// RSI Cycle 82: soft-stale cached Value for warm 15m RSI fires.
pub fn build_local_stratum_slice_for_wake(store: &StoreHandle) -> Value {
    if !enabled() {
        return json!({
            "version": "v1",
            "enabled": false,
            "node_count": 0,
            "nodes": [],
            "wake_lean": true,
            "sovereignty_note": "ENGRAM_LOCAL_STRATUM=off",
        });
    }

    let key = store.store_path().to_string();
    if let Some(cached) = local_wake_slice_cache_get(&key) {
        return cached;
    }

    // Core host only — profile + mcp. Readiness lives on wake packet `readiness` field.
    // C82: existence-only nodes (no ProvLog body) — sovereignty names sufficient on lean wake.
    let concepts = [LOCAL_HOST_PROFILE, LOCAL_HOST_MCP];
    let mut nodes: Vec<Value> = Vec::new();
    for c in concepts {
        let present =
            store.fetch_block_high_priority(c).is_some() || store.fetch_block(c).is_some();
        if present {
            nodes.push(json!({
                "concept": c,
                "preview": "",
                "crs": 0.0,
                "hot": true,
                "tier": "local_only",
            }));
        }
    }

    let out = json!({
        "version": "v1",
        "enabled": true,
        "wake_lean": true,
        "budget": 2,
        "node_count": nodes.len(),
        "soft_stale_cache": true,
        "sovereignty_note": "local_only blocks never export raw — use scrub_export when implemented",
        "process": "process:engram.ritual.local-context-working-memory",
        "nodes": nodes,
    });
    local_wake_slice_cache_set(&key, out.clone());
    out
}

/// Shorter previews for wake (unused on C82 existence-only path; kept for full-preview recovery).
#[allow(dead_code)]
fn preview_for_wake(store: &StoreHandle, concept: &str) -> Option<Value> {
    let block = store.fetch_block_high_priority(concept)?;
    let text = storage::read_provlog(&block);
    let preview: String = text
        .lines()
        .skip_while(|l| l.starts_with('#'))
        .take(2)
        .collect::<Vec<_>>()
        .join(" ");
    let preview = if preview.len() > 96 {
        format!("{}…", preview.chars().take(95).collect::<String>())
    } else if preview.is_empty() {
        text.chars().take(80).collect()
    } else {
        preview
    };
    let tier = if concept.starts_with("local:host:") {
        "local_only"
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::StoreHandle;

    /// RSI Cycle 82: second wake slice is soft-stale hit (near-instant, empty previews).
    #[test]
    fn wake_local_stratum_soft_stale_second_call() {
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        std::env::set_var("ENGRAM_FORCE_CPU_BACKEND", "1");
        std::env::set_var("ENGRAM_LOCAL_WAKE_SOFT_STALE_SECS", "1800");
        if let Ok(mut cache) = LOCAL_WAKE_SLICE_CACHE.lock() {
            cache.store_key.clear();
            cache.last_ok = None;
            cache.value = None;
        }
        let dir = std::env::temp_dir().join(format!(
            "engram_lcs_soft_{}_{}",
            std::process::id(),
            now_secs()
        ));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.to_string_lossy().to_string();
        let mut store = StoreHandle::new(&path);
        // Seed without full bootstrap (avoids readiness/BVH hang).
        let _ = store.remember(
            LOCAL_HOST_PROFILE,
            "# Local Host Profile\n\n**sovereignty:** local_only\n",
        );
        let _ = store.remember(
            LOCAL_HOST_MCP,
            "# Local MCP\n\n**sovereignty:** local_only\n",
        );
        let s1 = build_local_stratum_slice_for_wake(&store);
        assert_eq!(s1.get("wake_lean").and_then(|v| v.as_bool()), Some(true));
        assert!(s1
            .get("soft_stale_cache")
            .and_then(|v| v.as_bool())
            .unwrap_or(false));
        assert!(s1.get("node_count").and_then(|v| v.as_u64()).unwrap_or(0) >= 1);
        let t0 = std::time::Instant::now();
        let s2 = build_local_stratum_slice_for_wake(&store);
        assert!(
            t0.elapsed().as_millis() < 20,
            "soft-stale second call near-instant"
        );
        assert_eq!(
            s2.get("node_count").and_then(|v| v.as_u64()),
            s1.get("node_count").and_then(|v| v.as_u64())
        );
        let _ = std::fs::remove_dir_all(&dir);
        std::env::remove_var("ENGRAM_LOCAL_WAKE_SOFT_STALE_SECS");
    }

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

    /// RSI Cycle 62: wake local slice is profile+mcp only (no readiness_cache).
    #[test]
    fn wake_local_slice_core_only_skips_readiness() {
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        std::env::set_var("ENGRAM_FORCE_CPU_BACKEND", "1");
        let dir = std::env::temp_dir().join(format!("engram_lcs_wake_{}", now_secs()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.to_string_lossy().to_string();
        let mut lock = StoreHandle::new(&path);
        // Mint core locals without full bootstrap (avoids readiness upsert hang paths).
        let _ = lock.remember(
            LOCAL_HOST_PROFILE,
            "# Local Host Profile\n\n**sovereignty:** local_only\n",
        );
        let _ = lock.remember(
            LOCAL_HOST_MCP,
            "# Local MCP\n\n**sovereignty:** local_only\n",
        );
        let _ = lock.remember(
            LOCAL_HOST_READINESS,
            "# Local Readiness Cache\n\n```json\n{\"bvh_ready\":false}\n```\n",
        );
        lock.mark_hot(LOCAL_HOST_PROFILE);
        lock.mark_hot(LOCAL_HOST_MCP);
        lock.mark_hot(LOCAL_HOST_READINESS);
        let slice = build_local_stratum_slice_for_wake(&lock);
        assert_eq!(slice.get("wake_lean").and_then(|v| v.as_bool()), Some(true));
        let nodes = slice
            .get("nodes")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let names: Vec<String> = nodes
            .iter()
            .filter_map(|n| {
                n.get("concept")
                    .and_then(|c| c.as_str())
                    .map(|s| s.to_string())
            })
            .collect();
        assert!(
            names.iter().any(|n| n == LOCAL_HOST_PROFILE),
            "expected profile in wake slice: {names:?}"
        );
        assert!(
            !names.iter().any(|n| n == LOCAL_HOST_READINESS),
            "wake slice must skip readiness_cache: {names:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

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
