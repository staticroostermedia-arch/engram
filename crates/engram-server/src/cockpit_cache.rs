//! RAM cache for LEG cockpit endpoints — avoids rebuilding harness on every poll.
//!
//! Invalidates when `activity_feed.jsonl` mtime changes or TTL expires (2s default).

use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

static CACHE_HITS: AtomicU64 = AtomicU64::new(0);
static CACHE_MISSES: AtomicU64 = AtomicU64::new(0);

const DEFAULT_TTL: Duration = Duration::from_secs(2);

#[derive(Clone)]
struct Entry {
    payload: Value,
    built_at: Instant,
    activity_stamp: u64,
}

static CONTEXT_WINDOW_CACHE: Mutex<Option<Entry>> = Mutex::new(None);
static CONSCIOUSNESS_CACHE: Mutex<Option<Entry>> = Mutex::new(None);

/// Rolling hit rate for cockpit presentation endpoints (0.0 when no requests yet).
pub fn presentation_cache_hit_rate() -> f64 {
    let hits = CACHE_HITS.load(Ordering::Relaxed);
    let misses = CACHE_MISSES.load(Ordering::Relaxed);
    let total = hits + misses;
    if total == 0 {
        0.0
    } else {
        hits as f64 / total as f64
    }
}

pub fn cache_enabled() -> bool {
    !matches!(
        std::env::var("ENGRAM_PRESENTATION_CACHE")
            .unwrap_or_else(|_| "1".to_string())
            .to_ascii_lowercase()
            .as_str(),
        "0" | "false" | "off"
    )
}

fn activity_feed_stamp() -> u64 {
    let path =
        PathBuf::from(shellexpand::tilde("~/.engram").into_owned()).join("activity_feed.jsonl");
    std::fs::metadata(&path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn ttl() -> Duration {
    std::env::var("ENGRAM_PRESENTATION_CACHE_TTL_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(|ms| Duration::from_millis(ms.clamp(500, 30_000)))
        .unwrap_or(DEFAULT_TTL)
}

fn entry_fresh(entry: &Entry) -> bool {
    let stamp = activity_feed_stamp();
    entry.activity_stamp == stamp && entry.built_at.elapsed() < ttl()
}

fn get_cached(slot: &Mutex<Option<Entry>>) -> Option<Value> {
    if !cache_enabled() {
        return None;
    }
    let guard = slot.lock().ok()?;
    guard
        .as_ref()
        .filter(|e| entry_fresh(e))
        .map(|e| e.payload.clone())
}

fn put_cached(slot: &Mutex<Option<Entry>>, payload: Value) {
    if !cache_enabled() {
        return;
    }
    if let Ok(mut guard) = slot.lock() {
        *guard = Some(Entry {
            payload,
            built_at: Instant::now(),
            activity_stamp: activity_feed_stamp(),
        });
    }
}

pub fn invalidate_all() {
    if let Ok(mut g) = CONTEXT_WINDOW_CACHE.lock() {
        *g = None;
    }
    if let Ok(mut g) = CONSCIOUSNESS_CACHE.lock() {
        *g = None;
    }
}

pub fn context_window<F>(build: F) -> Value
where
    F: FnOnce() -> Value,
{
    if let Some(v) = get_cached(&CONTEXT_WINDOW_CACHE) {
        CACHE_HITS.fetch_add(1, Ordering::Relaxed);
        let mut out = v;
        if let Some(obj) = out.as_object_mut() {
            obj.insert("cache_hit".into(), Value::Bool(true));
        }
        return out;
    }
    CACHE_MISSES.fetch_add(1, Ordering::Relaxed);
    let payload = build();
    put_cached(&CONTEXT_WINDOW_CACHE, payload.clone());
    let mut out = payload;
    if let Some(obj) = out.as_object_mut() {
        obj.insert("cache_hit".into(), Value::Bool(false));
    }
    out
}

pub fn consciousness_surface<F>(build: F) -> Value
where
    F: FnOnce() -> Value,
{
    if let Some(v) = get_cached(&CONSCIOUSNESS_CACHE) {
        CACHE_HITS.fetch_add(1, Ordering::Relaxed);
        let mut out = v;
        if let Some(obj) = out.as_object_mut() {
            obj.insert("cache_hit".into(), Value::Bool(true));
        }
        return out;
    }
    CACHE_MISSES.fetch_add(1, Ordering::Relaxed);
    let payload = build();
    put_cached(&CONSCIOUSNESS_CACHE, payload.clone());
    let mut out = payload;
    if let Some(obj) = out.as_object_mut() {
        obj.insert("cache_hit".into(), Value::Bool(false));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_disabled_when_env_zero() {
        std::env::set_var("ENGRAM_PRESENTATION_CACHE", "0");
        assert!(!cache_enabled());
        std::env::remove_var("ENGRAM_PRESENTATION_CACHE");
    }
}
