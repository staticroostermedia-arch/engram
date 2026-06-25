//! Auto-extraction sidecar for `mcp_engram_turn_record` — episodic facts + graph edges.

use crate::store::StoreHandle;
/// Extract short episodic facts from a turn (no LLM — heuristic ADD-only).
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

pub fn turn_extract_enabled() -> bool {
    let v = std::env::var("ENGRAM_TURN_EXTRACT")
        .unwrap_or_else(|_| "1".to_string())
        .to_ascii_lowercase();
    !matches!(v.as_str(), "0" | "false" | "off")
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
    if !turn_extract_enabled() {
        return Vec::new();
    }

    let facts = extract_fact_lines(human_forward, user_utterance, assistant_output);
    let mut minted = Vec::new();

    for (i, fact) in facts.iter().enumerate() {
        let concept = format!("episodic:turn_{}_{}", ts, i);
        let text = format!(
            "EPISODIC TURN EXTRACT (auto)\n\n**source_tile:** {}\n**fact:** {}\n",
            tile_key, fact
        );
        if store.remember(&concept, &text).is_err() {
            continue;
        }
        let _ = store.relate(tile_key, &concept, "summarizes");
        if !goal_ctx.is_empty() {
            let _ = store.relate(goal_ctx, &concept, "documents");
        }
        minted.push(concept);
    }

    minted
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
