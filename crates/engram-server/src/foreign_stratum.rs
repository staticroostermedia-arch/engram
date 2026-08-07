//! E9 — External knowledge foreign stratum (low CRS, excluded from anchors).

use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::Mutex;

static FOREIGN: Mutex<Option<HashSet<String>>> = Mutex::new(None);
static ACCEPTED: Mutex<Option<HashSet<String>>> = Mutex::new(None);

fn with_foreign<R>(f: impl FnOnce(&mut HashSet<String>) -> R) -> R {
    let mut g = FOREIGN.lock().unwrap();
    if g.is_none() {
        *g = Some(HashSet::new());
    }
    f(g.as_mut().unwrap())
}

fn with_accepted<R>(f: impl FnOnce(&mut HashSet<String>) -> R) -> R {
    let mut g = ACCEPTED.lock().unwrap();
    if g.is_none() {
        *g = Some(HashSet::new());
    }
    f(g.as_mut().unwrap())
}

pub const FOREIGN_INITIAL_CRS: f32 = 0.55;

pub fn mint_external_concept(source_label: &str, path_or_label: &str) -> String {
    let slug: String = path_or_label
        .chars()
        .rev()
        .take_while(|c| *c != '/' && *c != '\\')
        .collect::<String>()
        .chars()
        .rev()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
        .take(48)
        .collect();
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("external:{source_label}:{slug}:{ts}")
}

pub fn register_foreign(concept: &str) {
    with_foreign(|s| {
        s.insert(concept.to_string());
    });
}

pub fn is_foreign(concept: &str) -> bool {
    with_foreign(|s| s.contains(concept))
}

pub fn accept_external(concept: &str) -> Value {
    if !is_foreign(concept) {
        return json!({"ok": false, "error": "not_foreign", "concept": concept});
    }
    with_accepted(|s| {
        s.insert(concept.to_string());
    });
    json!({
        "ok": true,
        "version": "foreign_stratum_v1",
        "accepted": concept,
        "crs_cap_lifted": true,
        "note": "eligible for pin/promote after verify",
    })
}

pub fn is_accepted(concept: &str) -> bool {
    with_accepted(|s| s.contains(concept))
}

/// Anchors default: omit foreign unless accepted.
pub fn filter_anchors_default(concepts: &[String], include_foreign: bool) -> Vec<String> {
    if include_foreign {
        return concepts.to_vec();
    }
    concepts
        .iter()
        .filter(|c| !is_foreign(c) || is_accepted(c))
        .cloned()
        .collect()
}

pub fn build_foreign_payload(source_label: &str, text: &str, path: &str) -> (String, String, f32) {
    let concept = mint_external_concept(source_label, path);
    let body = format!(
        "EXTERNAL KNOWLEDGE (foreign stratum)\n\n\
         **source:** {source_label}\n\
         **path:** {path}\n\
         **foreign:** true\n\
         **crs_initial:** {FOREIGN_INITIAL_CRS}\n\
         **source_tag:** source:external\n\n\
         ---\n\n{text}"
    );
    (concept, body, FOREIGN_INITIAL_CRS)
}

/// Local paths only by default (no SSRF).
pub fn path_allowed(path: &str) -> bool {
    if path.starts_with("http://") || path.starts_with("https://") {
        return std::env::var("ENGRAM_EXTERNAL_URL_FETCH").as_deref() == Ok("1");
    }
    true
}

#[cfg(test)]
pub fn reset_for_tests() {
    *FOREIGN.lock().unwrap() = Some(HashSet::new());
    *ACCEPTED.lock().unwrap() = Some(HashSet::new());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchors_exclude_foreign() {
        reset_for_tests();
        let (c, _, _) = build_foreign_payload("docs", "hello world", "/tmp/x.md");
        register_foreign(&c);
        let anchors =
            filter_anchors_default(&["goal:main".into(), c.clone(), "trace:1".into()], false);
        assert!(anchors.contains(&"goal:main".into()));
        assert!(!anchors.contains(&c));
    }

    #[test]
    fn accept_then_visible() {
        reset_for_tests();
        let (c, _, crs) = build_foreign_payload("docs", "body", "a.md");
        assert!(crs < 0.7);
        register_foreign(&c);
        assert_eq!(accept_external(&c)["ok"], true);
        let anchors = filter_anchors_default(&[c.clone()], false);
        assert!(anchors.contains(&c));
    }

    #[test]
    fn url_blocked_by_default() {
        assert!(!path_allowed("https://evil.example/x"));
        assert!(path_allowed("/home/a/doc.md"));
    }
}
