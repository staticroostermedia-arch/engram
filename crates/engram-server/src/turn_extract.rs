//! Auto-extraction sidecar for `mcp_engram_turn_record` — episodic facts + graph edges.

use crate::store::StoreHandle;

/// Mem0-style single-pass extraction backend (testable via mock).
pub trait FactExtractor: Send + Sync {
    fn extract(
        &self,
        human_forward: &str,
        user_utterance: &str,
        assistant_output: &str,
    ) -> Option<Vec<String>>;
}

/// Heuristic ADD-only fallback (no LLM).
pub fn extract_fact_lines(
    human_forward: &str,
    user_utterance: &str,
    assistant_output: &str,
) -> Vec<String> {
    let mut facts = Vec::new();
    let hf = human_forward.trim();
    if !hf.is_empty() && hf.len() <= 512 {
        facts.push(hf.to_string());
    }

    let user = user_utterance.trim();
    if !user.is_empty() && user.len() <= 280 {
        facts.push(format!("User: {}", user));
    } else if !user.is_empty() {
        let excerpt: String = user.chars().take(200).collect();
        facts.push(format!("User: {}…", excerpt));
    }

    for line in assistant_output.lines() {
        let t = line.trim();
        if t.len() < 12 || t.len() > 400 {
            continue;
        }
        let lower = t.to_lowercase();
        if lower.starts_with("- ")
            || lower.starts_with("* ")
            || lower.contains("implemented")
            || lower.contains("shipped")
            || lower.contains("decided")
            || lower.contains("fixed")
            || lower.contains("goal:")
            || t.contains(".rs")
            || t.contains("docs/")
        {
            facts.push(t.to_string());
        }
        if facts.len() >= 6 {
            break;
        }
    }

    facts.sort();
    facts.dedup();
    facts.truncate(5);
    facts
}

fn normalize_llm_facts(raw: &str) -> Vec<String> {
    let mut facts: Vec<String> = raw
        .lines()
        .map(|l| {
            l.trim()
                .trim_start_matches(|c: char| c == '-' || c == '*' || c.is_whitespace())
                .trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == ')')
                .trim()
                .to_string()
        })
        .filter(|l| !l.is_empty() && l.len() >= 8 && l.len() <= 400)
        .collect();
    facts.sort();
    facts.dedup();
    facts.truncate(5);
    facts
}

pub fn turn_llm_extract_enabled() -> bool {
    let v = std::env::var("ENGRAM_TURN_LLM_EXTRACT")
        .unwrap_or_else(|_| "1".to_string())
        .to_ascii_lowercase();
    !matches!(v.as_str(), "0" | "false" | "off")
}

pub fn turn_extract_enabled() -> bool {
    let v = std::env::var("ENGRAM_TURN_EXTRACT")
        .unwrap_or_else(|_| "1".to_string())
        .to_ascii_lowercase();
    !matches!(v.as_str(), "0" | "false" | "off")
}

/// OpenAI-compatible chat extractor (`ENGRAM_LLM_URL` or `ENGRAM_SCOUT_LLM_URL` base).
pub struct HttpLlmExtractor {
    client: reqwest::blocking::Client,
    url: String,
    model: String,
}

impl HttpLlmExtractor {
    pub fn from_env() -> Option<Self> {
        let base = std::env::var("ENGRAM_LLM_URL")
            .or_else(|_| std::env::var("ENGRAM_SCOUT_LLM_URL"))
            .ok()?;
        let base = base.trim_end_matches('/').to_string();
        let url = if base.ends_with("/v1/chat/completions") {
            base
        } else {
            format!("{base}/v1/chat/completions")
        };
        let model = std::env::var("ENGRAM_LLM_MODEL")
            .or_else(|_| std::env::var("ENGRAM_SCOUT_LLM_MODEL"))
            .unwrap_or_else(|_| "gemma4".to_string());
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .ok()?;
        Some(Self { client, url, model })
    }
}

impl FactExtractor for HttpLlmExtractor {
    fn extract(
        &self,
        human_forward: &str,
        user_utterance: &str,
        assistant_output: &str,
    ) -> Option<Vec<String>> {
        let prompt = format!(
            "Extract 3-5 salient episodic facts from this agent turn as short standalone statements (Mem0 ADD pass).\n\
             One fact per line, no bullets, no raw code dumps, normalized prose only.\n\n\
             human_forward:\n{human_forward}\n\nuser:\n{user_utterance}\n\nassistant:\n{assistant_output}"
        );
        let body = serde_json::json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": "You extract concise episodic memory facts. Output only fact lines."},
                {"role": "user", "content": prompt}
            ],
            "max_tokens": 512,
            "temperature": 0.2
        });
        let resp: serde_json::Value = self
            .client
            .post(&self.url)
            .json(&body)
            .send()
            .ok()?
            .json()
            .ok()?;
        let content = resp
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())?;
        let facts = normalize_llm_facts(content);
        if facts.is_empty() {
            None
        } else {
            Some(facts)
        }
    }
}

/// Select facts: LLM branch when enabled + reachable, else heuristic.
pub fn extract_facts_with_source(
    human_forward: &str,
    user_utterance: &str,
    assistant_output: &str,
    extractor: Option<&dyn FactExtractor>,
) -> (Vec<String>, &'static str) {
    if turn_llm_extract_enabled() {
        if let Some(ext) = extractor {
            if let Some(facts) = ext.extract(human_forward, user_utterance, assistant_output) {
                if !facts.is_empty() {
                    return (facts, "llm");
                }
            }
        } else if let Some(http) = HttpLlmExtractor::from_env() {
            if let Some(facts) = http.extract(human_forward, user_utterance, assistant_output) {
                if !facts.is_empty() {
                    return (facts, "llm");
                }
            }
        }
    }
    (
        extract_fact_lines(human_forward, user_utterance, assistant_output),
        "heuristic",
    )
}

/// Mint episodic blocks and wire relations into the navigation graph.
pub fn mint_turn_episodics(
    store: &mut StoreHandle,
    tile_key: &str,
    goal_ctx: &str,
    human_forward: &str,
    user_utterance: &str,
    assistant_output: &str,
    ts: u64,
) -> Vec<String> {
    mint_turn_episodics_with_extractor(
        store,
        tile_key,
        goal_ctx,
        human_forward,
        user_utterance,
        assistant_output,
        ts,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn mint_turn_episodics_with_extractor(
    store: &mut StoreHandle,
    tile_key: &str,
    goal_ctx: &str,
    human_forward: &str,
    user_utterance: &str,
    assistant_output: &str,
    ts: u64,
    extractor: Option<&dyn FactExtractor>,
) -> Vec<String> {
    if !turn_extract_enabled() {
        return Vec::new();
    }

    let (facts, source) =
        extract_facts_with_source(human_forward, user_utterance, assistant_output, extractor);
    let goal = if goal_ctx.is_empty() {
        crate::store::resolve_active_or_recent_goal(store).unwrap_or_default()
    } else {
        goal_ctx.to_string()
    };
    let mut minted = Vec::new();

    for (i, fact) in facts.iter().enumerate() {
        let concept = format!("episodic:turn_{}_{}", ts, i);
        let text = format!(
            "EPISODIC TURN EXTRACT (auto)\n\n**extraction:** {}\n**source_tile:** {}\n**fact:** {}\n",
            source, tile_key, fact
        );
        if store.remember(&concept, &text).is_err() {
            continue;
        }
        let _ = store.relate(tile_key, &concept, "summarizes");
        if !goal.is_empty() {
            let _ = store.relate(&goal, &concept, "documents");
        }
        let _ = store.auto_relate_after_write(&concept);
        minted.push(concept);
    }

    minted
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockLlmExtractor {
        facts: Vec<String>,
    }

    impl FactExtractor for MockLlmExtractor {
        fn extract(&self, _hf: &str, _user: &str, _out: &str) -> Option<Vec<String>> {
            if self.facts.is_empty() {
                None
            } else {
                Some(self.facts.clone())
            }
        }
    }

    #[test]
    fn extract_dedupes_and_caps() {
        let facts = extract_fact_lines(
            "Shipped relational recall",
            "complete the goal",
            "- Implemented cuFile gate\n- Implemented turn extract\n- docs/plans/foo.md updated",
        );
        assert!(!facts.is_empty());
        assert!(facts.len() <= 5);
    }

    #[test]
    fn turn_extract_toggle() {
        std::env::remove_var("ENGRAM_TURN_EXTRACT");
        assert!(turn_extract_enabled());
        std::env::set_var("ENGRAM_TURN_EXTRACT", "0");
        assert!(!turn_extract_enabled());
        std::env::remove_var("ENGRAM_TURN_EXTRACT");
    }

    #[test]
    fn llm_extract_branch_used_when_mock_returns_facts() {
        std::env::set_var("ENGRAM_TURN_LLM_EXTRACT", "1");
        let mock = MockLlmExtractor {
            facts: vec![
                "Merged relational-first lean v2 into master".to_string(),
                "Agent recall now walks the goal graph before geometric search".to_string(),
            ],
        };
        let (facts, src) = extract_facts_with_source(
            "summary",
            "merge the PR",
            "raw line dump should not appear",
            Some(&mock),
        );
        assert_eq!(src, "llm");
        assert_eq!(facts.len(), 2);
        assert!(!facts[0].starts_with('-'));
        std::env::remove_var("ENGRAM_TURN_LLM_EXTRACT");
    }

    #[test]
    fn heuristic_fallback_when_llm_disabled() {
        std::env::set_var("ENGRAM_TURN_LLM_EXTRACT", "0");
        let mock = MockLlmExtractor {
            facts: vec!["should be ignored".into()],
        };
        let (facts, src) = extract_facts_with_source(
            "Shipped relational recall",
            "complete",
            "- Implemented cuFile gate",
            Some(&mock),
        );
        assert_eq!(src, "heuristic");
        assert!(!facts.is_empty());
        std::env::remove_var("ENGRAM_TURN_LLM_EXTRACT");
    }

    #[test]
    fn normalize_llm_strips_bullets() {
        let facts = normalize_llm_facts(
            "1. First normalized fact here\n- Second fact about deployment\n* Third fact",
        );
        assert_eq!(facts.len(), 3);
        assert!(!facts[0].starts_with('-'));
    }

    #[test]
    fn mint_turn_episodics_llm_source_in_block() {
        use crate::store::StoreHandle;
        use std::time::{SystemTime, UNIX_EPOCH};

        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let dir = std::env::temp_dir().join(format!("engram_turn_extract_test_{ts}"));
        let _ = std::fs::remove_dir_all(&dir);
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember(
                "primary_goal",
                "PRIMARY GOAL\n\n**goal:** unset\n**cleared_after:** test\n",
            )
            .unwrap();
        store
            .remember(
                "goal:turn_extract_goal",
                "GOAL\n\n**status:** active\n**statement:** turn extract\n",
            )
            .unwrap();
        store.access_index.touch("goal:turn_extract_goal");

        std::env::set_var("ENGRAM_TURN_EXTRACT", "1");
        std::env::set_var("ENGRAM_TURN_LLM_EXTRACT", "1");
        let mock = MockLlmExtractor {
            facts: vec![
                "Relational recall now walks the presentation stratum graph".into(),
                "Auto-relate falls back to recent active goals when primary is unset".into(),
            ],
        };
        let minted = mint_turn_episodics_with_extractor(
            &mut store,
            "tile:agent_response_test",
            "",
            "Closed lean gaps",
            "implement LLM turn extract",
            "- raw bullet should not be stored",
            ts,
            Some(&mock),
        );
        assert_eq!(minted.len(), 2);
        let block = store
            .fetch_block_high_priority(&minted[0])
            .expect("episodic block");
        let text = crate::store::goal_block_text(&block);
        assert!(text.contains("**extraction:** llm"));
        assert!(text.contains("Relational recall"));
        assert!(!text.contains("raw bullet"));
        let edges = store.search_relations("goal:turn_extract_goal", Some("documents"), "from");
        assert!(edges.iter().any(|(_, c)| c.as_str() == minted[0].as_str()));
        std::env::remove_var("ENGRAM_TURN_EXTRACT");
        std::env::remove_var("ENGRAM_TURN_LLM_EXTRACT");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
