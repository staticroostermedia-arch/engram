//! E5 — Multi-agent single-key leases with TTL + conflict minting.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
struct Lease {
    agent_id: String,
    expires_ms: u64,
}

static LEASES: Mutex<Option<HashMap<String, Lease>>> = Mutex::new(None);
static CONFLICTS: Mutex<Option<Vec<Value>>> = Mutex::new(None);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn with_leases<R>(f: impl FnOnce(&mut HashMap<String, Lease>) -> R) -> R {
    let mut g = LEASES.lock().unwrap();
    if g.is_none() {
        *g = Some(HashMap::new());
    }
    f(g.as_mut().unwrap())
}

fn purge_expired(map: &mut HashMap<String, Lease>) {
    let n = now_ms();
    map.retain(|_, l| l.expires_ms > n);
}

pub fn lease_acquire(concept: &str, agent_id: &str, ttl_ms: u64) -> Value {
    let ttl_ms = ttl_ms.clamp(10, 3_600_000);
    with_leases(|m| {
        purge_expired(m);
        if let Some(existing) = m.get(concept) {
            if existing.agent_id != agent_id {
                return json!({
                    "ok": false,
                    "error": "lease_held",
                    "concept": concept,
                    "holder": existing.agent_id,
                    "expires_ms": existing.expires_ms,
                });
            }
        }
        let exp = now_ms().saturating_add(ttl_ms);
        m.insert(
            concept.to_string(),
            Lease {
                agent_id: agent_id.to_string(),
                expires_ms: exp,
            },
        );
        json!({
            "ok": true,
            "version": "lease_v1",
            "concept": concept,
            "agent_id": agent_id,
            "ttl_ms": ttl_ms,
            "expires_ms": exp,
        })
    })
}

pub fn lease_release(concept: &str, agent_id: &str) -> Value {
    with_leases(|m| {
        purge_expired(m);
        match m.get(concept) {
            Some(l) if l.agent_id == agent_id => {
                m.remove(concept);
                json!({"ok": true, "released": concept})
            }
            Some(l) => json!({
                "ok": false,
                "error": "not_holder",
                "holder": l.agent_id,
            }),
            None => json!({"ok": true, "released": concept, "note": "no_lease"}),
        }
    })
}

/// Admin break-glass.
pub fn lease_break(concept: &str) -> Value {
    with_leases(|m| {
        m.remove(concept);
        json!({"ok": true, "broken": concept, "break_glass": true})
    })
}

/// Check write permission; mint conflict if held by other.
/// Policy from ENGRAM_CONFLICT: refuse | mint_and_refuse (default).
pub fn check_write(concept: &str, agent_id: &str) -> Value {
    with_leases(|m| {
        purge_expired(m);
        match m.get(concept) {
            Some(l) if l.agent_id != agent_id => {
                let mode =
                    std::env::var("ENGRAM_CONFLICT").unwrap_or_else(|_| "mint_and_refuse".into());
                let conflict_id = format!("conflict:{}-{}", concept, now_ms());
                let conflict = json!({
                    "id": conflict_id,
                    "concept": concept,
                    "holder": l.agent_id,
                    "writer": agent_id,
                    "mode": mode,
                });
                {
                    let mut cg = CONFLICTS.lock().unwrap();
                    if cg.is_none() {
                        *cg = Some(Vec::new());
                    }
                    cg.as_mut().unwrap().push(conflict.clone());
                }
                json!({
                    "allowed": false,
                    "error": "lease_conflict",
                    "conflict": conflict,
                })
            }
            _ => json!({"allowed": true}),
        }
    })
}

pub fn list_conflicts() -> Vec<Value> {
    CONFLICTS.lock().unwrap().clone().unwrap_or_default()
}

#[cfg(test)]
pub fn reset_for_tests() {
    *LEASES.lock().unwrap() = Some(HashMap::new());
    *CONFLICTS.lock().unwrap() = Some(Vec::new());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn second_acquire_fails_during_ttl() {
        // Unique concept keys avoid parallel-test races on process-local maps.
        let c = format!("goal:lease_ttl_{}", std::process::id());
        let a = lease_acquire(&c, "agent_a", 5_000);
        assert_eq!(a["ok"], true);
        let b = lease_acquire(&c, "agent_b", 5_000);
        assert_eq!(b["ok"], false);
        assert_eq!(b["error"], "lease_held");
        let _ = lease_release(&c, "agent_a");
    }

    #[test]
    fn conflict_on_write() {
        let c = format!("tile:lease_conflict_{}", std::process::id());
        lease_acquire(&c, "a1", 5_000);
        let w = check_write(&c, "a2");
        assert_eq!(w["allowed"], false, "{w}");
        assert!(w.get("conflict").is_some());
        assert!(!list_conflicts().is_empty());
        let _ = lease_release(&c, "a1");
    }

    #[test]
    fn expire_then_reacquire() {
        let c = format!("metric:lease_exp_{}", std::process::id());
        lease_acquire(&c, "a1", 50);
        thread::sleep(Duration::from_millis(80));
        let b = lease_acquire(&c, "a2", 1_000);
        assert_eq!(b["ok"], true, "{b}");
        let _ = lease_release(&c, "a2");
    }

    #[test]
    fn break_glass() {
        let c = format!("goal:lease_stuck_{}", std::process::id());
        lease_acquire(&c, "a1", 60_000);
        let br = lease_break(&c);
        assert_eq!(br["break_glass"], true);
        assert_eq!(lease_acquire(&c, "a2", 1000)["ok"], true);
        let _ = lease_release(&c, "a2");
    }
}
