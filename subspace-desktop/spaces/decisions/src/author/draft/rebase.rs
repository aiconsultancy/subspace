//! Merge edits to collections whose write payload replaces the complete value.

use std::collections::BTreeSet;

use gpui_kit::component::input::InputState;
use gpui_kit::{Context, Entity, Window};
use subspace_api_client::model::DecisionDetail;

use super::{Draft, hidden_issues, issue_identifiers, issue_keys, tag_names};
use crate::Decisions;

impl Draft {
    pub(in crate::author) fn rebase_collections(
        &mut self,
        current: &DecisionDetail,
        window: &mut Window,
        cx: &mut Context<Decisions>,
    ) {
        let Some(original) = self.original.as_mut() else {
            return;
        };
        self.hidden_issues = hidden_issues(current);
        rebase_field(
            (&mut original.tags, &self.tags),
            current.tags.join(", "),
            tag_names,
            window,
            cx,
        );
        if !self.hidden_issues {
            rebase_field(
                (&mut original.issues, &self.issues),
                issue_identifiers(current),
                issue_keys,
                window,
                cx,
            );
        }
    }
}

fn rebase_field(
    draft: (&mut String, &Entity<InputState>),
    current: String,
    parse: fn(&str) -> Vec<String>,
    window: &mut Window,
    cx: &mut Context<Decisions>,
) {
    let (original, field) = draft;
    let merged = apply_intent(parse, original, &field.read(cx).value(), &current);
    *original = current;
    field.update(cx, |field, cx| field.set_value(merged, window, cx));
}

fn apply_intent(
    parse: fn(&str) -> Vec<String>,
    original: &str,
    draft: &str,
    current: &str,
) -> String {
    let set = |text: &str| parse(text).into_iter().collect::<BTreeSet<_>>();
    let original = set(original);
    let draft = set(draft);
    let current_set = set(current);
    let removed = &original - &draft;
    let added = &draft - &original;
    let merged: BTreeSet<_> = (&current_set - &removed).union(&added).cloned().collect();
    if merged == current_set {
        current.to_owned()
    } else {
        merged.into_iter().collect::<Vec<_>>().join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::{apply_intent, issue_keys, tag_names};

    #[test]
    fn use_current_version_reapplies_only_the_drafts_collection_changes() {
        assert_eq!(
            apply_intent(issue_keys, "EX-1, EX-2", "ex-1, Ex-3", "EX-2, EX-4"),
            "EX-3, EX-4"
        );
        assert_eq!(
            apply_intent(tag_names, "late, early", "early, late", "late, early, new"),
            "late, early, new"
        );
    }
}
