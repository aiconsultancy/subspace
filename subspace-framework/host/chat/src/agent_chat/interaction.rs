use super::ingestion::{
    AttachmentBudget, AttachmentResult, DroppedAttachment, IngestionBatch, add_image, ingest_batch,
    ingest_source, path_batch,
};
use super::input::ChatInput;
use super::logic::upsert_context;
use super::slash::SlashList;
use super::tabs::tab_index;
use super::toolbar::ChatToolbar;
use super::{
    AgentHandle, App, ChatEngine, ChatId, ChatPanel, Context, Entity, POLL_INTERVAL,
    PathPromptOptions, Task, Window, prefs, services,
};
use gpui_kit::{AppContext as _, ExternalPaths};
use std::sync::Arc;
use subspace_agent::{ChatPreferences, ChatToolServer};

const INGESTION_BUSY_ERROR: &str = "Wait for the current attachments to finish before adding more.";

impl ChatPanel {
    pub(super) fn drop_paths(&mut self, paths: &ExternalPaths, cx: &mut Context<Self>) {
        let Some((origin, budget)) = self.begin_ingestion() else {
            self.log_accepted_errors(cx);
            cx.notify();
            return;
        };
        let batch = path_batch(paths.0.iter().cloned());
        self.start_ingestion(origin, budget, batch, cx);
    }

    pub(super) fn paste_attachments(&mut self, batch: &IngestionBatch, cx: &mut Context<Self>) {
        let Some((origin, budget)) = self.begin_ingestion() else {
            self.log_accepted_errors(cx);
            cx.notify();
            return;
        };
        self.start_ingestion(origin, budget, batch.clone(), cx);
    }

    fn begin_ingestion(&mut self) -> Option<(IngestionOrigin, AttachmentBudget)> {
        if self.attachment_ingestion_busy {
            self.tab_mut().set_error(INGESTION_BUSY_ERROR.into());
            return None;
        }
        self.attachment_ingestion_busy = true;
        Some((
            IngestionOrigin {
                chat: self.tab().id,
                runtime: self.handle.clone(),
            },
            AttachmentBudget::from_images(&self.tab().images),
        ))
    }

    fn start_ingestion(
        &mut self,
        origin: IngestionOrigin,
        budget: AttachmentBudget,
        batch: IngestionBatch,
        cx: &mut Context<Self>,
    ) {
        if batch.sources.is_empty() {
            self.finish_ingestion(&origin, Vec::new(), batch.notice);
            self.log_accepted_errors(cx);
            cx.notify();
            return;
        }
        let worker_origin = origin.clone();
        cx.spawn(async move |chat, cx| {
            let results = cx
                .background_spawn(async move {
                    ingest_batch(batch.sources, budget, |source, byte_limit| {
                        ingest_source(worker_origin.runtime.as_ref(), source, byte_limit)
                    })
                })
                .await;
            let _ = chat.update(cx, |chat, cx| {
                chat.finish_ingestion(&origin, results, batch.notice);
                chat.log_accepted_errors(cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn finish_ingestion(
        &mut self,
        origin: &IngestionOrigin,
        results: Vec<AttachmentResult>,
        notice: Option<String>,
    ) {
        self.attachment_ingestion_busy = false;
        if !self.origin_is_open(origin) {
            return;
        }
        self.apply_ingestion(origin.chat, results);
        if let (Some(index), Some(notice)) = (tab_index(&self.tabs, origin.chat), notice) {
            self.tabs[index].set_error(notice);
        }
    }

    fn origin_is_open(&self, origin: &IngestionOrigin) -> bool {
        let chat_open = tab_index(&self.tabs, origin.chat).is_some();
        let same_runtime = match (&origin.runtime, &self.handle) {
            (Some(origin), Some(current)) => origin.same_runtime(current),
            (None, None) => true,
            _ => false,
        };
        chat_open && same_runtime
    }

    fn apply_ingestion(&mut self, chat_id: ChatId, results: Vec<AttachmentResult>) {
        let Some(index) = tab_index(&self.tabs, chat_id) else {
            return;
        };
        for result in results {
            match result {
                Ok(DroppedAttachment::Text(context)) => {
                    upsert_context(&mut self.tabs[index].context, context);
                }
                Ok(DroppedAttachment::Image(image)) => {
                    if let Err(error) = add_image(&mut self.tabs[index].images, image) {
                        self.tabs[index].set_error(error);
                    }
                }
                Err(error) => self.tabs[index].set_error(error),
            }
        }
    }

    pub(crate) fn remove_image_draft(&mut self, id: u64) -> bool {
        let tab = self.tab_mut();
        let before = tab.images.len();
        tab.images.retain(|image| image.id != id);
        if let Some(queued) = &mut tab.queued {
            queued.images.retain(|image| image.id != id);
        }
        tab.images.len() != before
    }

    pub(super) fn attach(&mut self, cx: &mut Context<Self>) {
        let Some((origin, budget)) = self.begin_ingestion() else {
            self.log_accepted_errors(cx);
            cx.notify();
            return;
        };
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Attach context files or images".into()),
        });
        cx.spawn(async move |chat, cx| match paths.await {
            Ok(Ok(Some(paths))) => {
                let _ = chat.update(cx, |chat, cx| {
                    if chat.origin_is_open(&origin) {
                        chat.start_ingestion(origin, budget, path_batch(paths), cx);
                    } else {
                        chat.attachment_ingestion_busy = false;
                    }
                });
            }
            Ok(Err(error)) => {
                let _ = chat.update(cx, |chat, cx| {
                    chat.attachment_ingestion_busy = false;
                    if chat.origin_is_open(&origin) {
                        if let Some(index) = tab_index(&chat.tabs, origin.chat) {
                            let chain = format!("{error:#}");
                            chat.tabs[index].set_error_chain(error.to_string(), chain);
                        }
                        chat.log_accepted_errors(cx);
                        cx.notify();
                    }
                });
            }
            _ => {
                let _ = chat.update(cx, |chat, _| {
                    chat.attachment_ingestion_busy = false;
                });
            }
        })
        .detach();
    }
}

#[derive(Clone)]
struct IngestionOrigin {
    chat: ChatId,
    runtime: Option<AgentHandle>,
}

pub(super) fn chat_preferences(cx: &App) -> ChatPreferences {
    ChatPreferences {
        copilot_model: prefs::get(cx, "chat.default-model.copilot"),
        pi_model: prefs::get(cx, "chat.default-model.pi"),
        acp_model: prefs::get(cx, "chat.default-model.acp"),
        approve_all: prefs::get(cx, "chat.approve-all-default").as_deref() == Some("1"),
    }
}

pub(super) fn new_input(
    engine: ChatEngine,
    agent: subspace_agent::AcpAgent,
    toolbar: &Entity<ChatToolbar>,
    window: &mut Window,
    cx: &mut Context<ChatPanel>,
) -> Entity<ChatInput> {
    let name = super::engine::engine_label(engine, agent);
    let input = cx.new(|cx| ChatInput::new(name, window, cx));
    let addon = toolbar.clone().into();
    input.update(cx, |input, cx| input.set_addon(Some(addon), cx));
    input
}

pub(super) fn new_toolbar(window: &mut Window, cx: &mut Context<ChatPanel>) -> Entity<ChatToolbar> {
    cx.new(|cx| ChatToolbar::new(window, cx))
}

pub(super) fn new_slash(
    engine: ChatEngine,
    window: &mut Window,
    cx: &mut Context<ChatPanel>,
) -> Entity<SlashList> {
    cx.new(|cx| SlashList::new(engine, window, cx))
}

pub(super) fn polling_task(cx: &mut Context<ChatPanel>) -> Task<()> {
    cx.spawn(async move |chat, cx| {
        loop {
            cx.background_executor().timer(POLL_INTERVAL).await;
            if chat.update(cx, |chat, cx| chat.poll(cx)).is_err() {
                break;
            }
        }
    })
}

pub(super) fn start_runtime(
    chat_tools: Option<Arc<dyn ChatToolServer>>,
    cx: &App,
) -> Result<AgentHandle, String> {
    let preferences = chat_preferences(cx);
    services(cx)
        .map(|services| {
            AgentHandle::start_with_preferences(
                services.config.clone(),
                services.credentials.clone(),
                services.tools.clone(),
                preferences,
                chat_tools,
            )
        })
        .unwrap_or_else(|| Err("Service configuration is unavailable.".into()))
}

#[cfg(test)]
mod tests {
    use super::{INGESTION_BUSY_ERROR, chat_preferences};
    use crate::agent_chat::ChatPanel;
    use crate::agent_chat::REVEAL_DURATION;
    use crate::agent_chat::ingestion::{
        AttachmentBudget, AttachmentSource, DroppedAttachment, MAX_DRAFT_IMAGES, add_image,
        clipboard_batch, clipboard_batch_with, ingest_batch, path_batch,
    };
    use crate::agent_chat::tabs::Tab;
    use crate::prefs;
    use gpui_kit::component::input::Paste;
    use gpui_kit::{
        ClipboardEntry, ClipboardItem, ExternalPaths, FileDropEvent, Image, ImageFormat,
        InputEvent as _, TestAppContext, point, px,
    };
    use image::{ImageBuffer, ImageFormat as EncodedFormat, Rgba};
    use std::cell::Cell;
    use std::io::Cursor;
    use std::path::PathBuf;
    use subspace_agent::{DRAFT_ATTACHMENT_LIMIT, ImageAttachment};

    fn image(id: u64, size: usize) -> ImageAttachment {
        ImageAttachment {
            id,
            data: vec![0; size].into(),
            mime_type: "image/png".into(),
            display_name: format!("{id}.png"),
        }
    }

    fn png() -> Vec<u8> {
        let image = ImageBuffer::from_pixel(2, 1, Rgba([255_u8, 20, 10, 255]));
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, EncodedFormat::Png).unwrap();
        bytes.into_inner()
    }

    #[test]
    fn clipboard_preserves_copied_paths_and_images_within_the_batch_limit() {
        let item = ClipboardItem {
            entries: vec![
                ClipboardEntry::ExternalPaths(ExternalPaths(
                    [PathBuf::from("one.txt"), PathBuf::from("two.png")]
                        .into_iter()
                        .collect(),
                )),
                ClipboardEntry::Image(Image::from_bytes(ImageFormat::Png, vec![1, 2, 3])),
            ],
        };
        assert_eq!(
            clipboard_batch(
                &item,
                AttachmentBudget::new(MAX_DRAFT_IMAGES, DRAFT_ATTACHMENT_LIMIT),
            )
            .sources
            .len(),
            3
        );
    }

    #[test]
    fn oversized_clipboard_image_is_rejected_before_its_bytes_are_cloned() {
        let item = ClipboardItem::new_image(&Image::from_bytes(
            ImageFormat::Png,
            vec![0; crate::images::MAX_ENCODED_BYTES + 1],
        ));
        let clones = Cell::new(0);
        let batch = clipboard_batch_with(
            &item,
            AttachmentBudget::new(MAX_DRAFT_IMAGES, DRAFT_ATTACHMENT_LIMIT),
            |bytes| {
                clones.set(clones.get() + 1);
                bytes.into()
            },
        );
        assert!(batch.sources.is_empty());
        assert!(batch.notice.is_some());
        assert_eq!(clones.get(), 0);
    }

    #[test]
    fn batch_never_ingests_images_after_count_or_byte_budget_is_exhausted() {
        let byte_limited_sources = (0..8)
            .map(|index| AttachmentSource::Path(PathBuf::from(format!("{index}.png"))))
            .collect();
        let byte_limited_calls = Cell::new(0);
        let byte_limited_results =
            ingest_batch(byte_limited_sources, AttachmentBudget::new(8, 2), |_, _| {
                byte_limited_calls.set(byte_limited_calls.get() + 1);
                Ok(DroppedAttachment::Image(image(1, 2)))
            });
        let count_limited_sources = (0..8)
            .map(|index| AttachmentSource::Path(PathBuf::from(format!("{index}.png"))))
            .collect();
        let count_limited_calls = Cell::new(0);
        let count_limited_results = ingest_batch(
            count_limited_sources,
            AttachmentBudget::new(1, 100),
            |_, _| {
                count_limited_calls.set(count_limited_calls.get() + 1);
                Ok(DroppedAttachment::Image(image(1, 1)))
            },
        );
        assert_eq!(byte_limited_calls.get(), 1);
        assert_eq!(count_limited_calls.get(), 1);
        assert_eq!(byte_limited_results.len(), 8);
        assert_eq!(count_limited_results.len(), 8);
        assert_eq!(
            byte_limited_results
                .iter()
                .filter(|result| result.is_ok())
                .count(),
            1
        );
        assert_eq!(
            count_limited_results
                .iter()
                .filter(|result| result.is_ok())
                .count(),
            1
        );
    }

    #[test]
    fn source_and_result_queues_are_bounded() {
        let batch = path_batch((0..100).map(|index| PathBuf::from(format!("{index}.txt"))));
        assert_eq!(batch.sources.len(), MAX_DRAFT_IMAGES);
        assert!(batch.notice.is_some());
        let results = ingest_batch(
            batch.sources,
            AttachmentBudget::new(MAX_DRAFT_IMAGES, DRAFT_ATTACHMENT_LIMIT),
            |_, _| Err("not readable".into()),
        );
        assert_eq!(results.len(), MAX_DRAFT_IMAGES);
    }

    #[test]
    fn image_drafts_have_count_and_total_memory_bounds() {
        let mut images = (0..MAX_DRAFT_IMAGES as u64)
            .map(|id| image(id, 1))
            .collect::<Vec<_>>();
        assert!(add_image(&mut images, image(99, 1)).is_err());
        let mut images = vec![image(1, DRAFT_ATTACHMENT_LIMIT)];
        assert!(add_image(&mut images, image(2, 1)).is_err());
    }

    #[gpui_kit::test]
    fn actual_chat_drop_adds_image_to_originating_draft(cx: &mut TestAppContext) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("dropped.png");
        std::fs::write(&path, png()).unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            prefs::init_file(None, cx);
        });
        let (panel, cx) = cx.add_window_view(ChatPanel::new);
        // Animations follow the test clock: open the panel fully first.
        cx.executor().advance_clock(REVEAL_DURATION);
        cx.update(|window, cx| {
            let _ = window.draw(cx);
            let position = point(px(10.), px(10.));
            window.dispatch_event(
                FileDropEvent::Entered {
                    position,
                    paths: ExternalPaths([path].into_iter().collect()),
                }
                .to_platform_input(),
                cx,
            );
            window.dispatch_event(FileDropEvent::Submit { position }.to_platform_input(), cx);
        });
        cx.run_until_parked();
        assert_eq!(
            cx.update(|_, cx| panel.read(cx).tab().images.len()),
            1,
            "the real on_drop handler should create a draft without sending"
        );
    }

    #[gpui_kit::test]
    fn repeated_ingestion_is_rejected_while_one_batch_is_in_flight(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            prefs::init_file(None, cx);
        });
        let (panel, cx) = cx.add_window_view(ChatPanel::new);
        cx.update(|_, cx| {
            panel.update(cx, |panel, _| {
                assert!(panel.begin_ingestion().is_some());
                assert!(panel.begin_ingestion().is_none());
                assert_eq!(panel.tab().error.as_deref(), Some(INGESTION_BUSY_ERROR));
            });
        });
    }

    #[gpui_kit::test]
    fn actual_textarea_paste_adds_image_to_chat_draft(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            prefs::init_file(None, cx);
        });
        let (panel, cx) = cx.add_window_view(ChatPanel::new_copilot);
        cx.update(|window, cx| {
            let input = panel.read(cx).input.clone();
            input.update(cx, |input, cx| input.focus(window, cx));
            let _ = window.draw(cx);
            cx.write_to_clipboard(ClipboardItem::new_image(&Image::from_bytes(
                ImageFormat::Png,
                png(),
            )));
            window.dispatch_action(Box::new(Paste), cx);
        });
        cx.run_until_parked();
        assert_eq!(cx.update(|_, cx| panel.read(cx).tab().images.len()), 1);
    }

    #[gpui_kit::test]
    fn completed_ingestion_never_moves_to_the_tab_selected_later(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            prefs::init_file(None, cx);
        });
        let (panel, cx) = cx.add_window_view(ChatPanel::new);
        cx.update(|_, cx| {
            panel.update(cx, |panel, _| {
                let origin = panel.tabs[0].id;
                panel.tabs.push(Tab::new(42));
                panel.active = 1;
                panel.apply_ingestion(origin, vec![Ok(DroppedAttachment::Image(image(7, 4)))]);
                assert_eq!(panel.tabs[0].images.len(), 1);
                assert!(panel.tabs[1].images.is_empty());
            });
        });
    }

    #[gpui_kit::test]
    fn completed_ingestion_is_discarded_for_a_closed_origin_or_replaced_runtime(
        cx: &mut TestAppContext,
    ) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            prefs::init_file(None, cx);
        });
        let (panel, cx) = cx.add_window_view(ChatPanel::new);
        let directory = tempfile::tempdir().unwrap();
        let (first, _first_commands) =
            subspace_agent::AgentHandle::test_offline(directory.path().into(), Vec::new());
        let (second, _second_commands) =
            subspace_agent::AgentHandle::test_offline(directory.path().into(), Vec::new());
        cx.update(|_, cx| {
            panel.update(cx, |panel, _| {
                panel.handle = Some(first);
                let (closed_origin, _) = panel.begin_ingestion().unwrap();
                panel.tabs = vec![Tab::new(42)];
                panel.finish_ingestion(
                    &closed_origin,
                    vec![Ok(DroppedAttachment::Image(image(1, 4)))],
                    Some("late error".into()),
                );
                assert!(panel.tab().images.is_empty());
                assert!(panel.tab().error.is_none());
                assert!(!panel.attachment_ingestion_busy);
                let (replaced_origin, _) = panel.begin_ingestion().unwrap();
                panel.handle = Some(second);
                panel.finish_ingestion(
                    &replaced_origin,
                    vec![Ok(DroppedAttachment::Image(image(2, 4)))],
                    Some("stale error".into()),
                );
                assert!(panel.tab().images.is_empty());
                assert!(panel.tab().error.is_none());
                assert!(!panel.attachment_ingestion_busy);
            })
        });
    }

    #[gpui_kit::test]
    fn failed_command_delivery_retains_the_image_draft(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            prefs::init_file(None, cx);
        });
        let (panel, cx) = cx.add_window_view(ChatPanel::new_copilot);
        cx.update(|window, cx| {
            panel.update(cx, |panel, cx| {
                panel.tab_mut().ready();
                panel.tab_mut().images.push(image(9, 4));
                panel.submit("describe it".into(), window, cx);
                assert_eq!(panel.tab().images.len(), 1);
                assert!(panel.tab().error.is_some());
            });
        });
    }

    #[gpui_kit::test]
    fn removal_updates_the_draft_and_a_message_waiting_for_startup(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            prefs::init_file(None, cx);
        });
        let (panel, cx) = cx.add_window_view(ChatPanel::new);
        cx.update(|_, cx| {
            panel.update(cx, |panel, _| {
                let attachment = image(11, 4);
                panel.tab_mut().images.push(attachment.clone());
                panel.tab_mut().queued = Some(crate::agent_chat::tabs::Queued {
                    text: "later".into(),
                    context: Vec::new(),
                    images: vec![attachment],
                });
                assert!(panel.remove_image_draft(11));
                assert!(panel.tab().images.is_empty());
                assert!(panel.tab().queued.as_ref().unwrap().images.is_empty());
            });
        });
    }

    #[gpui_kit::test]
    fn invalid_or_missing_approval_preferences_fail_closed(cx: &mut gpui_kit::TestAppContext) {
        cx.update(|cx| {
            prefs::init_file(None, cx);
            assert!(!chat_preferences(cx).approve_all);
            prefs::set(cx, "chat.approve-all-default", "unexpected");
            assert!(!chat_preferences(cx).approve_all);
            prefs::set(cx, "chat.approve-all-default", "1");
            assert!(chat_preferences(cx).approve_all);
        });
    }
}
