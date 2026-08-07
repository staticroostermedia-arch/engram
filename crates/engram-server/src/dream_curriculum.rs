//! E4 — Dream curriculum: offline self-test of manifold recall.

use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub struct DreamProbe {
    pub concept: String,
    pub query: String,
}

#[derive(Debug, Clone)]
pub struct DreamHit {
    pub concept: String,
    pub score: f32,
}

/// Score one probe: exact concept in top-k hits?
pub fn score_probe(probe: &DreamProbe, hits: &[DreamHit], k: usize) -> (bool, f32) {
    let k = k.max(1);
    let top: Vec<&DreamHit> = hits.iter().take(k).collect();
    let exact = top.iter().any(|h| h.concept == probe.concept);
    let best_crs = top.first().map(|h| h.score).unwrap_or(0.0);
    (exact, best_crs)
}

pub fn run_dream(probes: &[(DreamProbe, Vec<DreamHit>)], k: usize) -> Value {
    let mut hits = 0usize;
    let mut worst: Vec<Value> = Vec::new();
    let mut sum_crs = 0.0f32;
    for (probe, ranked) in probes {
        let (ok, crs) = score_probe(probe, ranked, k);
        sum_crs += crs;
        if ok {
            hits += 1;
        } else {
            worst.push(json!({
                "concept": probe.concept,
                "query": probe.query,
                "top": ranked.first().map(|h| h.concept.clone()),
            }));
        }
    }
    let n = probes.len().max(1);
    let accuracy = hits as f64 / n as f64;
    let mean_top_crs = sum_crs / n as f32;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    json!({
        "version": "dream_curriculum_v1",
        "metric_concept": format!("metric:dream_{ts}"),
        "probes": n,
        "hits": hits,
        "accuracy": accuracy,
        "mean_top_crs": mean_top_crs,
        "k": k,
        "worst_misses": worst.into_iter().take(10).collect::<Vec<_>>(),
        "last_dream_score": accuracy,
    })
}

/// Auto-schedule allowed only on fat profiles (not minimal).
pub fn dream_auto_schedule_enabled(host_profile: &str) -> bool {
    if std::env::var("ENGRAM_DREAM_AUTO").as_deref() == Ok("0") {
        return false;
    }
    if std::env::var("ENGRAM_DREAM_AUTO").as_deref() == Ok("1") {
        return !matches!(host_profile, "minimal" | "host_minimal");
    }
    // Default: off everywhere; explicit enable required
    false
}

pub fn metric_block_text(result: &Value) -> String {
    format!(
        "DREAM CURRICULUM METRIC\n\n{}\n",
        serde_json::to_string_pretty(result).unwrap_or_default()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_accuracy_measurable() {
        let probes = vec![
            (
                DreamProbe {
                    concept: "goal:a".into(),
                    query: "goal a".into(),
                },
                vec![
                    DreamHit {
                        concept: "goal:a".into(),
                        score: 0.9,
                    },
                    DreamHit {
                        concept: "goal:b".into(),
                        score: 0.5,
                    },
                ],
            ),
            (
                DreamProbe {
                    concept: "goal:b".into(),
                    query: "goal b".into(),
                },
                vec![DreamHit {
                    concept: "goal:c".into(),
                    score: 0.4,
                }],
            ),
        ];
        let r = run_dream(&probes, 3);
        assert_eq!(r["hits"], 1);
        assert!((r["accuracy"].as_f64().unwrap() - 0.5).abs() < 1e-9);
        assert!(r["metric_concept"]
            .as_str()
            .unwrap()
            .starts_with("metric:dream_"));
        assert!(!r["worst_misses"].as_array().unwrap().is_empty());
    }

    #[test]
    fn minimal_auto_off() {
        assert!(!dream_auto_schedule_enabled("minimal"));
        // default off even for dual
        assert!(!dream_auto_schedule_enabled("cuda_dual"));
    }
}
