//! Per-process fulfillment metrics from sheaf TOMLs + realized_by graph edges (WS-3).

use serde_json::{json, Value};
use std::path::Path;

#[derive(Default)]
struct OutcomeCounts {
    trace: usize,
    tile: usize,
    praxis: usize,
    scar: usize,
    by_pattern: std::collections::HashMap<String, usize>,
}

/// Glob-like match: `trace:*_subvisor` matches `trace:123_subvisor_enforce`.
pub fn glob_match(pattern: &str, concept: &str) -> bool {
    if !pattern.contains('*') {
        return pattern == concept;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.is_empty() {
        return false;
    }
    let mut pos = 0usize;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i == 0 {
            if !concept.starts_with(part) {
                return false;
            }
            pos = part.len();
        } else if i == parts.len() - 1 {
            if !concept.ends_with(part) {
                return false;
            }
        } else {
            match concept[pos..].find(part) {
                Some(idx) => pos += idx + part.len(),
                None => return false,
            }
        }
    }
    true
}

fn category_from_concept(concept: &str) -> &'static str {
    if concept.starts_with("trace:") {
        "trace"
    } else if concept.starts_with("tile:") {
        "tile"
    } else if concept.starts_with("praxis") || concept.starts_with("praxis:") {
        "praxis"
    } else if concept.starts_with("scar:") {
        "scar"
    } else {
        "other"
    }
}

/// Load `[produces].list` for a process key from processes/ TOMLs.
pub fn load_process_produces(process_key: &str, processes_dir: &Path) -> (String, Vec<String>) {
    let agent_name = process_key.replace("process:engram.", "agent:engram.");
    let subdirs = [
        "ritual",
        "harness",
        "operator",
        "monitor",
        "process",
        "linguistic",
        "meta",
    ];
    for sub in &subdirs {
        let dir = processes_dir.join(sub);
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                    continue;
                }
                let Ok(content) = std::fs::read_to_string(&path) else {
                    continue;
                };
                let Ok(value) = toml::from_str::<toml::Value>(&content) else {
                    continue;
                };
                let raw_name = value
                    .get("process")
                    .and_then(|p| p.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let key = if raw_name.starts_with("agent:engram.") {
                    raw_name.replace("agent:engram.", "process:engram.")
                } else if raw_name.is_empty() {
                    continue;
                } else {
                    format!("process:{}", raw_name)
                };
                if key == process_key || raw_name == agent_name {
                    let produces: Vec<String> = value
                        .get("produces")
                        .and_then(|v| v.get("list"))
                        .and_then(|v| v.as_array())
                        .map(|a| {
                            a.iter()
                                .filter_map(|vv| vv.as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default();
                    return (path.to_string_lossy().into_owned(), produces);
                }
            }
        }
    }
    (String::new(), Vec::new())
}

/// Build metrics JSON for a process key.
pub fn build_process_metrics(
    process_key: &str,
    realized_by_concepts: &[String],
    all_concepts: &[String],
    processes_dir: &Path,
) -> Value {
    let (toml_path, declared_produces) = load_process_produces(process_key, processes_dir);

    let mut outcomes = OutcomeCounts::default();
    for concept in all_concepts {
        for pattern in &declared_produces {
            if glob_match(pattern, concept) {
                *outcomes.by_pattern.entry(pattern.clone()).or_insert(0) += 1;
                match category_from_concept(concept) {
                    "trace" => outcomes.trace += 1,
                    "tile" => outcomes.tile += 1,
                    "praxis" => outcomes.praxis += 1,
                    "scar" => outcomes.scar += 1,
                    _ => {}
                }
            }
        }
    }

    let realized_by_count = realized_by_concepts.len();
    let pattern_total: usize = outcomes.by_pattern.values().sum();
    let fulfillment_ratio = if declared_produces.is_empty() {
        0.0
    } else {
        let matched_patterns = outcomes.by_pattern.len();
        matched_patterns as f64 / declared_produces.len() as f64
    };

    json!({
        "process_key": process_key,
        "toml": toml_path,
        "declared_produces": declared_produces,
        "outcomes": {
            "trace": { "count": outcomes.trace },
            "tile": { "count": outcomes.tile },
            "praxis": { "count": outcomes.praxis },
            "scar": { "count": outcomes.scar },
            "by_pattern": outcomes.by_pattern,
            "pattern_total": pattern_total,
        },
        "realized_by_count": realized_by_count,
        "realized_by_sample": realized_by_concepts.iter().take(8).collect::<Vec<_>>(),
        "fulfillment_ratio": fulfillment_ratio,
    })
}

/// Parse `processes/meta/*.toml` workflow name from `[workflow].name`.
#[allow(dead_code)]
pub fn parse_meta_workflow_name(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let value = toml::from_str::<toml::Value>(&content).ok()?;
    value
        .get("workflow")
        .and_then(|w| w.get("name"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn full_system_audit_loop_toml_parses() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../processes/meta/full_system_audit_loop.toml");
        let name = parse_meta_workflow_name(&path)
            .unwrap_or_else(|| panic!("must parse {}", path.display()));
        assert_eq!(name, "full_system_audit_loop");
        let content = std::fs::read_to_string(&path).unwrap();
        let v: toml::Value = toml::from_str(&content).unwrap();
        assert!(
            v.get("execute").is_some(),
            "meta workflow must have execute steps"
        );
    }

    #[test]
    fn glob_match_wildcard_suffix() {
        assert!(glob_match(
            "trace:*_subvisor_enforce",
            "trace:123_subvisor_enforce"
        ));
        assert!(!glob_match("trace:*_subvisor_enforce", "trace:123_other"));
    }

    #[test]
    fn glob_match_exact() {
        assert!(glob_match("goal:test", "goal:test"));
        assert!(!glob_match("goal:test", "goal:other"));
    }
}
