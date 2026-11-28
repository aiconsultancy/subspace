//! Retains each unsent reply, and a bounded set of recently sent lines.

use std::collections::{HashMap, HashSet, VecDeque};

use gpui_kit::{App, AppContext as _, Context, Entity, Subscription, Window};
use subspace_client_irc::BufferKey;

use super::{Recall, ReplyDraft};
use crate::conversation::Conversation;
use crate::state::IrcState;

/// Recently used threads whose sent history survives retiring the editor.
const KEPT_HISTORIES: usize = 12;

#[derive(Default)]
pub(crate) struct Drafts {
    by_root: HashMap<String, Entity<ReplyDraft>>,
    opened: Vec<String>,
    recent: VecDeque<(String, Recall)>,
    selected: Option<Selected>,
}

struct Selected {
    root: String,
    seq: i64,
    _observe: Subscription,
}

impl Drafts {
    pub(crate) fn get(&self, root: &str) -> Option<Entity<ReplyDraft>> {
        self.by_root.get(root).cloned()
    }

    pub(crate) fn remove(&mut self, root: &str) {
        if self
            .selected
            .as_ref()
            .is_some_and(|selected| selected.root == root)
        {
            self.selected = None;
        }
        self.opened.retain(|opened| opened != root);
        self.by_root.remove(root);
    }

    pub(crate) fn selected_seq(&self) -> Option<i64> {
        self.selected.as_ref().map(|selected| selected.seq)
    }

    pub(crate) fn resumable(&self, cx: &App) -> Option<&str> {
        self.opened.iter().rev().find_map(|root| {
            self.by_root
                .get(root)?
                .read(cx)
                .has_text(cx)
                .then_some(root.as_str())
        })
    }

    pub(crate) fn revalidate(&self, cx: &mut App) {
        if let Some(draft) = self
            .selected
            .as_ref()
            .and_then(|selected| self.get(&selected.root))
        {
            draft.update(cx, ReplyDraft::revalidate);
        }
    }

    fn activate(&mut self, draft: Entity<ReplyDraft>, selection: Selected, cx: &mut App) {
        let root = &selection.root;
        self.opened.retain(|opened| opened != root);
        self.opened.push(root.clone());
        self.by_root.insert(root.clone(), draft);
        let previous = self.selected.replace(selection);
        if let Some(previous) = previous
            && self
                .selected
                .as_ref()
                .is_some_and(|selected| selected.root != previous.root)
        {
            self.retire(previous.root, cx);
        }
    }

    fn retire(&mut self, root: String, cx: &mut App) {
        let Some(draft) = self.get(&root) else { return };
        if draft.read(cx).has_text(cx) {
            return;
        }
        let recall = draft.update(cx, |draft, _| draft.recall.take_sent());
        self.remove(&root);
        self.remember(root, recall);
    }

    fn remember(&mut self, root: String, recall: Recall) {
        if !recall.is_empty() {
            self.recent.push_back((root, recall));
            if self.recent.len() > KEPT_HISTORIES {
                self.recent.pop_front();
            }
        }
    }

    fn recall(&mut self, root: &str) -> Recall {
        let Some(index) = self.recent.iter().position(|(id, _)| id == root) else {
            return Recall::default();
        };
        self.recent
            .remove(index)
            .map(|(_, recall)| recall)
            .unwrap_or_default()
    }

    pub(crate) fn missing(
        &self,
        buffer: &BufferKey,
        state: &IrcState,
        cx: &App,
    ) -> Vec<Entity<ReplyDraft>> {
        let written: Vec<_> = self
            .by_root
            .iter()
            .filter(|(_, draft)| draft.read(cx).has_text(cx))
            .collect();
        if written.is_empty() {
            return Vec::new();
        }
        let loaded = loaded_ids(buffer, state);
        let mut missing: Vec<_> = written
            .into_iter()
            .filter(|(root, _)| !loaded.contains(root.as_str()))
            .collect();
        missing.sort_unstable_by_key(|(root, _)| *root);
        missing
            .into_iter()
            .map(|(_, draft)| draft.clone())
            .collect()
    }
}

fn loaded_ids<'a>(buffer: &BufferKey, state: &'a IrcState) -> HashSet<&'a str> {
    state
        .buffer(buffer)
        .into_iter()
        .flat_map(|held| held.entries.iter())
        .filter_map(|entry| entry.line.id.as_deref())
        .collect()
}

pub(crate) fn select(
    conversation: &mut Conversation,
    target: (&str, i64),
    window: &mut Window,
    cx: &mut Context<Conversation>,
) -> Entity<ReplyDraft> {
    let (root, seq) = target;
    let draft = conversation.reply_drafts.get(root).unwrap_or_else(|| {
        let buffer = conversation.buffer.clone();
        let hub = conversation.hub.clone();
        let recall = conversation.reply_drafts.recall(root);
        let draft = cx.new(|cx| ReplyDraft::new(buffer, root.to_owned(), hub, window, cx));
        draft.update(cx, |draft, _| draft.recall = recall);
        draft
    });
    let selected = Selected {
        root: root.to_owned(),
        seq,
        _observe: cx.observe(&draft, |_, _, cx| cx.notify()),
    };
    conversation
        .reply_drafts
        .activate(draft.clone(), selected, cx);
    conversation.reply_drafts.revalidate(cx);
    cx.notify();
    draft
}

#[cfg(test)]
mod tests {
    use super::{Drafts, KEPT_HISTORIES, Recall};

    #[test]
    fn retired_editors_keep_only_recent_sent_lines() {
        let mut drafts = Drafts::default();
        for index in 0..=KEPT_HISTORIES {
            let message = index.to_string();
            let mut recall = Recall::default();
            recall.push(message.clone());
            drafts.remember(message, recall);
        }
        assert_eq!(drafts.recent.len(), KEPT_HISTORIES);
        assert!(drafts.recall("0").is_empty());
        let latest = KEPT_HISTORIES.to_string();
        assert_eq!(
            drafts.recall(&latest).older("").as_deref(),
            Some(latest.as_str())
        );
    }
}
