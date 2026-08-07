//! E6 — Selective sync mesh packs (export/import with quarantine CRS).

use serde_json::{json, Value};
use std::fs;
use std::path::Path;

pub const SCHEMA: &str = "engram_sync_pack_v1";
pub const QUARANTINE_CRS_CAP: f32 = 0.6;

#[derive(Debug, Clone)]
pub struct PackItem {
    pub concept: String,
    pub text: String,
    pub crs: f32,
    pub kind: String,
}

pub fn build_manifest(source_host: &str, items: &[PackItem], goal_id: &str) -> Value {
    let hashes: Vec<Value> = items
        .iter()
        .map(|i| {
            json!({
                "concept": i.concept,
                "bytes": i.text.len(),
                "crs": i.crs,
                "kind": i.kind,
            })
        })
        .collect();
    json!({
        "schema": SCHEMA,
        "source_host": source_host,
        "created_at": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        "goal_id": goal_id,
        "item_count": items.len(),
        "items": hashes,
        "version": SCHEMA,
    })
}

pub fn write_pack(out_dir: &Path, manifest: &Value, items: &[PackItem]) -> Result<(), String> {
    fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
    let man_path = out_dir.join("manifest.json");
    fs::write(
        &man_path,
        serde_json::to_string_pretty(manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let items_dir = out_dir.join("items");
    fs::create_dir_all(&items_dir).map_err(|e| e.to_string())?;
    for (idx, it) in items.iter().enumerate() {
        let safe: String = it
            .concept
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .take(80)
            .collect();
        let p = items_dir.join(format!("{idx:04}_{safe}.json"));
        let body = json!({
            "concept": it.concept,
            "text": it.text,
            "crs": it.crs,
            "kind": it.kind,
            "source": "sync",
        });
        fs::write(&p, serde_json::to_string_pretty(&body).unwrap()).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn validate_manifest(v: &Value) -> Result<(), String> {
    let schema = v
        .get("schema")
        .and_then(|s| s.as_str())
        .ok_or_else(|| "missing schema".to_string())?;
    if schema != SCHEMA {
        return Err(format!("unsupported schema: {schema}"));
    }
    if v.get("items").and_then(|i| i.as_array()).is_none() {
        return Err("missing items array".into());
    }
    Ok(())
}

pub fn read_pack(dir: &Path) -> Result<(Value, Vec<PackItem>), String> {
    let man_raw = fs::read_to_string(dir.join("manifest.json")).map_err(|e| e.to_string())?;
    let man: Value = serde_json::from_str(&man_raw).map_err(|e| e.to_string())?;
    validate_manifest(&man)?;
    let items_dir = dir.join("items");
    let mut items = Vec::new();
    if items_dir.is_dir() {
        let mut paths: Vec<_> = fs::read_dir(&items_dir)
            .map_err(|e| e.to_string())?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("json"))
            .collect();
        paths.sort();
        for p in paths {
            let raw = fs::read_to_string(&p).map_err(|e| e.to_string())?;
            let v: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
            items.push(PackItem {
                concept: v
                    .get("concept")
                    .and_then(|c| c.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
                text: v
                    .get("text")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string(),
                crs: v.get("crs").and_then(|c| c.as_f64()).unwrap_or(0.5) as f32,
                kind: v
                    .get("kind")
                    .and_then(|k| k.as_str())
                    .unwrap_or("block")
                    .to_string(),
            });
        }
    }
    Ok((man, items))
}

/// Quarantine CRS: min(original, cap).
pub fn quarantine_crs(original: f32) -> f32 {
    original.min(QUARANTINE_CRS_CAP)
}

pub fn import_summary(items: &[PackItem], quarantine: bool) -> Value {
    let mapped: Vec<Value> = items
        .iter()
        .map(|i| {
            let crs = if quarantine {
                quarantine_crs(i.crs)
            } else {
                i.crs
            };
            json!({
                "concept": i.concept,
                "crs": crs,
                "quarantine": quarantine,
                "source": "sync",
                "foreign": true,
            })
        })
        .collect();
    json!({
        "version": SCHEMA,
        "imported": mapped.len(),
        "items": mapped,
        "quarantine": quarantine,
        "crs_cap": QUARANTINE_CRS_CAP,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn roundtrip_export_import() {
        let dir = PathBuf::from(format!(
            "/tmp/grok-goal-f5ea8d6f3ccc/implementer/sync-pack-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        let items = vec![PackItem {
            concept: "goal:fixture".into(),
            text: "GOAL\n\n**status:** active\n".into(),
            crs: 0.91,
            kind: "goal".into(),
        }];
        let man = build_manifest("a-monad", &items, "goal:fixture");
        write_pack(&dir, &man, &items).unwrap();
        let (man2, items2) = read_pack(&dir).unwrap();
        assert_eq!(man2["schema"], SCHEMA);
        assert_eq!(items2.len(), 1);
        assert_eq!(items2[0].concept, "goal:fixture");
        let sum = import_summary(&items2, true);
        let crs = sum["items"][0]["crs"].as_f64().unwrap();
        assert!((crs - 0.6).abs() < 1e-5, "quarantine crs cap: {crs}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn malformed_rejected() {
        let dir = PathBuf::from(format!(
            "/tmp/grok-goal-f5ea8d6f3ccc/implementer/sync-bad-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("manifest.json"), r#"{"schema":"nope"}"#).unwrap();
        assert!(read_pack(&dir).is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
