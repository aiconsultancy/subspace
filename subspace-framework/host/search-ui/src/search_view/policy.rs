use crate::full_text::{Policy, Query, Ranking, SourceStatus};

const MAX_QUERY_BYTES: usize = 2048;
const RESULT_LIMIT: usize = 100;
const BYTES_PER_GB: f64 = 1_000_000_000.;
const MIN_QUOTA_BYTES: f64 = 1.;

pub(super) fn participates(sources: &[SourceStatus], board: &str) -> bool {
    sources.iter().any(|source| source.board == board)
}

pub(super) fn query(text: &str, current: &str, all: bool, rerank: bool) -> Result<Query, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Enter words to search cached content.".into());
    }
    if text.len() > MAX_QUERY_BYTES {
        return Err("Use a shorter query (at most 2,048 bytes).".into());
    }
    if !all && current.is_empty() {
        return Err("Choose a board or search all boards.".into());
    }
    Ok(Query {
        text: text.to_owned(),
        boards: if all {
            vec![]
        } else {
            vec![current.to_owned()]
        },
        limit: RESULT_LIMIT,
        ranking: if rerank {
            Ranking::Reranked
        } else {
            Ranking::Bm25
        },
    })
}

pub(super) fn draft_policy(enabled: bool, days: &str, gb: &str) -> Result<Policy, String> {
    let retention_days = days
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|days| *days > 0)
        .ok_or("Retention must be a positive whole number of days.")?;
    let gb = gb
        .trim()
        .parse::<f64>()
        .map_err(|_| "Enter a positive storage limit in GB.")?;
    let bytes = gb * BYTES_PER_GB;
    if !bytes.is_finite() || bytes < MIN_QUOTA_BYTES || bytes >= u64::MAX as f64 {
        return Err("Enter a positive storage limit in GB within the supported range.".into());
    }
    Ok(Policy {
        enabled,
        retention_days,
        max_bytes: bytes as u64,
    })
}

pub(super) fn step(selected: Option<usize>, count: usize, down: bool) -> Option<usize> {
    if count == 0 {
        return None;
    }
    Some(match selected {
        None => {
            if down {
                0
            } else {
                count - 1
            }
        }
        Some(index) if down => index.saturating_add(1).min(count - 1),
        Some(index) => index.saturating_sub(1).min(count - 1),
    })
}

#[cfg(test)]
mod tests {
    use super::{draft_policy, participates, query, step};
    use crate::full_text::SourceStatus;

    #[test]
    fn current_scope_uses_engine_participation_not_labels_or_opt_in_state() {
        let sources = [SourceStatus {
            board: "irc".into(),
            enabled: false,
            account: None,
            records: 0,
            bytes: 0,
            message: "Caching disabled".into(),
        }];
        assert!(participates(&sources, "irc"));
        assert!(!participates(&sources, "github-copilot-agents"));
        assert!(!participates(&[], "irc"));
        let all = !participates(&sources, "github-copilot-agents");
        assert!(
            query("deployment", "github-copilot-agents", all, false)
                .unwrap()
                .boards
                .is_empty()
        );
    }
    #[test]
    fn query_scope_is_explicit_and_unicode_limits_are_bytes() {
        assert_eq!(
            query(" café ", "irc", false, false).unwrap().boards,
            ["irc"]
        );
        assert!(query("café", "irc", true, false).unwrap().boards.is_empty());
        assert!(query(&"é".repeat(1025), "irc", false, false).is_err());
        assert!(query(" ", "irc", false, false).is_err());
    }
    #[test]
    fn quota_validation_rejects_non_finite_zero_and_overflow() {
        for value in ["NaN", "inf", "-1", "0", "1e30"] {
            assert!(draft_policy(true, "60", value).is_err());
        }
        assert!(draft_policy(true, "0", "2").is_err());
        assert_eq!(
            draft_policy(false, "60", "0.5").unwrap().max_bytes,
            500_000_000
        );
    }
    #[test]
    fn selection_handles_empty_and_shorter_result_sets() {
        assert_eq!(step(Some(3), 0, true), None);
        assert_eq!(step(None, 3, false), Some(2));
        assert_eq!(step(Some(7), 2, true), Some(1));
        assert_eq!(step(Some(0), 2, false), Some(0));
    }
}
