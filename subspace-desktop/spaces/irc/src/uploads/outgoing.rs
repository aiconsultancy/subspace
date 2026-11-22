//! The composer's attachment tray: up to [`MAX_ITEMS`] files wait above the
//! message input as thumbnails, Enter uploads them all in tray order and
//! then sends the text with every reference. Any failed upload sends
//! nothing to IRC and keeps the tray for a retry.
mod delivery;
mod follow;
mod recovery;
mod send;
mod source;
mod view;
pub(crate) use delivery::submit;
pub(crate) use follow::{dismiss, follow};
pub(crate) use recovery::check;
pub(crate) use source::{ingest, paths};
pub(crate) use view::{VISIBILITY, controls};

use super::intake::Preview;
use crate::conversation::Conversation;
use gpui_kit::{AnyWindowHandle, Context, Window};
use subspace_api_client::chat_uploads::{Upload, UploadDraft, UploadError};
use subspace_framework::dashboard::chat_input::attachments::AttachmentBudget;

/// Why the tray waits: a file is still being read.
pub(crate) const PREPARING: &str = "Wait for the attachments to be ready.";
/// Attachments one message can carry.
pub(crate) const MAX_ITEMS: usize = 10;
/// The total size of one message's attachments: the one-upload limit,
/// 25,000,000 bytes (decimal 25 MB), shared by every file in the tray.
pub(crate) const TOTAL_BYTES: usize = subspace_api_client::chat_uploads::MAX_BYTES;
/// Shown when a file would pass the count or total size limit.
pub(crate) const LIMIT_NOTICE: &str = "Attach up to 10 files, 25 MB in total, per message.";

/// The tray of one conversation's composer.
#[derive(Default)]
pub(crate) struct Tray {
    /// Keeps uploads' session demand alive while the tray holds anything.
    lease: Option<std::sync::Arc<()>>,
    pub(crate) items: Vec<Item>,
    pub(crate) phase: Phase,
    /// The last limit notice, until the tray changes again.
    pub(crate) notice: Option<&'static str>,
    next: usize,
}

/// One attachment in the tray, with an identity stable for its lifetime.
pub(crate) struct Item {
    pub(crate) id: usize,
    pub(crate) state: ItemState,
    /// Composer text moved into this attachment by "Send as attachment…".
    pub(crate) origin: Option<String>,
}

pub(crate) enum ItemState {
    /// Being read; once read it waits here until every earlier file is
    /// settled, so the total is checked in tray order.
    Preparing(Option<Box<Pending>>),
    Ready(Box<Pending>),
    Failed(UploadError),
}

/// A prepared file and its upload, once made.
pub(crate) struct Pending {
    scope: Option<super::session::Scope>,
    pub(crate) draft: UploadDraft,
    pub(crate) preview: Preview,
    pub(crate) uploaded: Option<Upload>,
    pub(crate) busy: bool,
    /// The upload may have completed: "Check upload" decides.
    pub(crate) uncertain: bool,
    pub(crate) error: Option<UploadError>,
}

/// Where the tray's send is.
#[derive(Default)]
pub(crate) enum Phase {
    #[default]
    Idle,
    /// Uploading in tray order; `text` goes with the references.
    Uploading { text: String },
    /// The lines went to the runtime as one `SendAll`, `id`.
    /// Once the runtime accepts them, the tray clears when every key
    /// echoes, or on Dismiss; a refusal returns the tray to `Idle` with the
    /// draft. Nothing is ever sent again from here.
    Queued {
        text: String,
        keys: Vec<String>,
        window: AnyWindowHandle,
        id: u64,
        accepted: bool,
    },
}

impl Pending {
    /// Drops the upload and its session, keeping the file, so the next
    /// explicit send uploads it again. The new upload takes a new ID: the
    /// old one may stay taken by an expired or deleted upload.
    fn forget_upload(&mut self) {
        self.uploaded = None;
        self.scope = None;
        self.draft = self.draft.renewed();
    }
}

impl Tray {
    pub(crate) fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub(crate) fn idle(&self) -> bool {
        matches!(self.phase, Phase::Idle)
    }

    /// Bytes the admitted files hold.
    fn bytes(&self) -> usize {
        self.items
            .iter()
            .map(|item| match &item.state {
                ItemState::Ready(p) => p.draft.bytes().len(),
                _ => 0,
            })
            .sum()
    }

    /// What a paste may still add.
    pub(crate) fn budget(&self) -> AttachmentBudget {
        AttachmentBudget::new(
            MAX_ITEMS.saturating_sub(self.items.len()),
            TOTAL_BYTES.saturating_sub(self.bytes()),
        )
        .with_image_limit(subspace_api_client::chat_uploads::MAX_BYTES)
    }

    /// A new item, if the count allows one.
    fn push(&mut self, state: ItemState, origin: Option<String>) -> Option<usize> {
        if self.items.len() >= MAX_ITEMS {
            self.notice = Some(LIMIT_NOTICE);
            return None;
        }
        let id = self.next;
        self.next += 1;
        self.items.push(Item { id, state, origin });
        Some(id)
    }

    pub(crate) fn pending_mut(&mut self, id: &str) -> Option<&mut Pending> {
        self.items
            .iter_mut()
            .find_map(|item| match &mut item.state {
                ItemState::Ready(p) if p.draft.id() == id => Some(p.as_mut()),
                _ => None,
            })
    }

    /// An empty tray needs no session and has nothing to send.
    fn settle(&mut self) {
        if self.items.is_empty() {
            self.lease = None;
            self.phase = Phase::Idle;
        }
    }

    /// Whether `draft` fits the total size limit beside what is there.
    fn fits(&self, draft: &UploadDraft) -> bool {
        self.bytes().saturating_add(draft.bytes().len()) <= TOTAL_BYTES
    }

    /// Admits read files in tray order: each takes its place while the
    /// total allows, otherwise it leaves with the limit notice. A file still
    /// being read holds back every file after it.
    fn admit(&mut self) {
        let mut at = 0;
        while at < self.items.len() {
            let used = self.bytes();
            match &mut self.items[at].state {
                ItemState::Preparing(None) => return,
                ItemState::Preparing(slot @ Some(_)) => {
                    let len = slot.as_ref().map_or(0, |p| p.draft.bytes().len());
                    if used.saturating_add(len) > TOTAL_BYTES {
                        self.items.remove(at);
                        self.notice = Some(LIMIT_NOTICE);
                        continue;
                    }
                    if let Some(p) = slot.take() {
                        self.items[at].state = ItemState::Ready(p);
                    }
                }
                _ => {}
            }
            at += 1;
        }
    }
}

fn pending(draft: UploadDraft, preview: Preview) -> Box<Pending> {
    Box::new(Pending {
        scope: None,
        draft,
        preview,
        uploaded: None,
        busy: false,
        uncertain: false,
        error: None,
    })
}

/// "Send as attachment…": the composer's text becomes a Markdown attachment
/// in the tray, and the composer clears.
pub(crate) fn text(this: &mut Conversation, window: &mut Window, cx: &mut Context<Conversation>) {
    let text = this.draft.entity.read(cx).text(cx);
    if text.trim().is_empty() || !this.tray.idle() {
        return;
    }
    // The total is checked in tray order, so wait for earlier files.
    let preparing = this
        .tray
        .items
        .iter()
        .any(|item| matches!(item.state, ItemState::Preparing(_)));
    if preparing {
        error(this, PREPARING, cx);
        return;
    }
    let state = match UploadDraft::new(
        "message.md".into(),
        "text/markdown".into(),
        text.as_bytes().to_vec(),
    ) {
        Ok(draft) if this.tray.fits(&draft) => {
            ItemState::Ready(pending(draft, Preview::Text(text.clone())))
        }
        Ok(_) => {
            this.tray.notice = Some(LIMIT_NOTICE);
            changed(this, cx);
            return;
        }
        Err(e) => ItemState::Failed(e),
    };
    hold(this, cx);
    if this.tray.push(state, Some(text)).is_some() {
        this.draft
            .entity
            .update(cx, |input, cx| input.clear(window, cx));
    }
    changed(this, cx);
}

/// × on a thumbnail. Text moved out of the composer goes back when the
/// composer holds nothing but blanks.
pub(crate) fn remove(
    this: &mut Conversation,
    id: usize,
    window: &mut Window,
    cx: &mut Context<Conversation>,
) {
    if !this.tray.idle() {
        return;
    }
    let Some(at) = this.tray.items.iter().position(|item| item.id == id) else {
        return;
    };
    let item = this.tray.items.remove(at);
    this.tray.notice = None;
    if let Some(origin) = item.origin
        && this.draft.entity.read(cx).text(cx).trim().is_empty()
    {
        this.draft
            .entity
            .update(cx, |input, cx| input.set_text(&origin, window, cx));
    }
    changed(this, cx);
}

/// Takes the uploads session's demand while the tray holds anything.
fn hold(this: &mut Conversation, cx: &mut Context<Conversation>) {
    if this.tray.lease.is_none() {
        this.tray.lease = Some(this.hub.update(cx, |hub, _| hub.uploads.demand.hold()));
    }
}

/// Keeps the composer in step with the tray: an empty draft sends while
/// the tray has files, and a paste may only add what is left.
pub(crate) fn changed(this: &mut Conversation, cx: &mut Context<Conversation>) {
    this.tray.settle();
    let submit_empty = !this.tray.is_empty();
    let budget = this.tray.budget();
    this.draft.entity.update(cx, |input, cx| {
        input.set_submit_empty(submit_empty, cx);
        input.set_attachment_budget(budget, cx);
    });
    crate::compose::revalidate(this, cx);
    cx.notify();
}

fn error(this: &mut Conversation, message: &str, cx: &mut Context<Conversation>) {
    this.draft
        .entity
        .update(cx, |input, cx| input.set_error(message.to_owned(), cx));
    cx.notify();
}
