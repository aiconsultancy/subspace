pub(super) mod rows;
use crate::Issues;
use gpui_kit::{AppContext as _, Context, IntoElement as _, PathPromptOptions};
use std::{
    io::{Cursor, Read as _},
    sync::Arc,
};
use subspace_api_client::uploads::{ImageDeclaration, MAX_IMAGE_BYTES, UploadError};
use subspace_framework::dashboard::chat_input::attachments::{
    AttachmentSource, IngestionBatch, path_batch,
};
const MAX_EDGE: u32 = 4096;
const MAX_PIXELS: u64 = 16_777_216;
const MAX_DECODE_BYTES: u64 = 64 * 1024 * 1024;

// qual:allow(coupling, oi) reason: "Native picker, clipboard leases and background preflight are coordinated at this UI lifecycle boundary."
impl Issues {
    pub(super) fn install_comment_attachments(
        &self,
        entity: &gpui_kit::Entity<subspace_framework::dashboard::composer::Composer>,
        parent: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let owner = self.weak.clone();
        let origin = (self.authoring.scope, self.authoring.issue.clone());
        entity.update(cx, |composer, cx| {
            composer.set_accessory(
                move |_, cx| {
                    owner
                        .update(cx, |this, cx| {
                            if origin != (this.authoring.scope, this.authoring.issue.clone()) {
                                return gpui_kit::div().into_any_element();
                            }
                            let draft = this
                                .hub
                                .as_ref()
                                .zip(this.authoring.issue.as_ref())
                                .and_then(|(hub, issue)| {
                                    hub.comment_draft(issue, parent.as_deref())
                                });
                            rows::rows(
                                this,
                                parent.as_deref(),
                                draft.as_ref(),
                                &subspace_framework::dashboard::Tokens::current(cx),
                                cx,
                            )
                        })
                        .unwrap_or_else(|_| gpui_kit::div().into_any_element())
                },
                cx,
            )
        });
    }
    fn image_target(
        &self,
        parent: Option<&str>,
    ) -> Option<(
        subspace_api_client::hub::Hub,
        String,
        subspace_api_client::hub::ScopeGeneration,
    )> {
        let (hub, issue) = self.hub.clone().zip(self.authoring.issue.clone())?;
        if hub.comment_reason(&issue, parent).is_some()
            || hub
                .comment_draft(&issue, parent)
                .is_some_and(|d| d.is_frozen())
        {
            return None;
        }
        Some((hub, issue, self.authoring.scope?))
    }
    pub(super) fn subscribe_comment_images(
        &self,
        entity: &gpui_kit::Entity<subspace_framework::dashboard::composer::Composer>,
        parent: Option<String>,
        cx: &mut Context<Self>,
    ) -> gpui_kit::Subscription {
        let origin = (self.authoring.scope, self.authoring.issue.clone());
        cx.subscribe(
            entity,
            move |this,
                  _,
                  event: &subspace_framework::dashboard::composer::ComposerImagePaste,
                  cx| {
                if origin == (this.authoring.scope, this.authoring.issue.clone()) {
                    this.ingest_comment_images(parent.clone(), event.0.clone(), cx);
                }
            },
        )
    }
    pub(super) fn attach_comment_images(&mut self, parent: Option<String>, cx: &mut Context<Self>) {
        let origin = (self.authoring.scope, self.authoring.issue.clone());
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Attach files".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await {
                _ = this.update(cx, |this, cx| {
                    if origin == (this.authoring.scope, this.authoring.issue.clone()) {
                        this.ingest_comment_images(parent, path_batch(paths), cx);
                    }
                });
            }
        })
        .detach();
    }

    pub(super) fn ingest_comment_images(
        &mut self,
        parent: Option<String>,
        batch: IngestionBatch,
        cx: &mut Context<Self>,
    ) {
        if self.authoring.intake_busy {
            self.authoring.intake_error = Some("Wait for these images to finish preparing.".into());
            cx.notify();
            return;
        }
        let Some((hub, issue, generation)) = self.image_target(parent.as_deref()) else {
            return;
        };
        let Ok(intake) = hub.begin_comment_image_intake(generation, &issue, parent.as_deref())
        else {
            return;
        };
        let origin = (generation, issue.clone());
        self.authoring.intake_busy = true;
        cx.spawn(async move |this, cx| {
            let error = cx
                .background_spawn(async move {
                    let mut error = batch.notice.map(|_| "Some attachments were omitted because they exceed the image or draft limits.".to_string());
                    if batch.sources.len() > subspace_api_client::uploads::MAX_DRAFT_IMAGES {
                        error = Some(UploadError::Budget.to_string());
                    }
                    for source in batch
                        .sources
                        .into_iter()
                        .take(subspace_api_client::uploads::MAX_DRAFT_IMAGES)
                    {
                        if !intake.current() {
                            break;
                        }
                        if let Err(failure) = ingest(source)
                            .and_then(|(declaration, bytes)| intake.attach(declaration, bytes))
                        {
                            error = Some(failure.to_string());
                        }
                    }
                    error
                })
                .await;
            _ = this.update(cx, |this, cx| {
                if this.authoring.scope == Some(origin.0)
                    && this.authoring.issue.as_ref() == Some(&origin.1)
                {
                    this.authoring.intake_error = error;
                    this.authoring.intake_busy = false;
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

type Intake = Result<(ImageDeclaration, Arc<[u8]>), UploadError>;

fn ingest(source: AttachmentSource) -> Intake {
    match source {
        AttachmentSource::Path(path) => {
            if !path.metadata().is_ok_and(|metadata| metadata.is_file()) {
                return Err(UploadError::InvalidImage);
            }
            let mime =
                subspace_api_client::uploads::file_mime(&path).ok_or(UploadError::InvalidImage)?;
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or(UploadError::InvalidImage)?;
            let file = std::fs::File::open(&path).map_err(|_| UploadError::InvalidImage)?;
            let mut bytes = Vec::new();
            file.take(MAX_IMAGE_BYTES as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| UploadError::InvalidImage)?;
            preflight(name, mime, bytes.into())
        }
        AttachmentSource::ClipboardImage {
            bytes,
            mime_type,
            display_name,
        } => clipboard_image(&display_name, mime_type, bytes),
    }
}

fn clipboard_image(name: &str, mime: &str, bytes: Arc<[u8]>) -> Intake {
    if matches!(
        mime,
        "image/png" | "image/jpeg" | "image/gif" | "image/webp"
    ) {
        return preflight(name, mime, bytes);
    }
    if !matches!(
        mime,
        "image/bmp" | "image/tiff" | "image/x-icon" | "image/x-portable-anymap"
    ) || bytes.len() > MAX_IMAGE_BYTES
    {
        return Err(UploadError::InvalidImage);
    }
    let mut reader = image::ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|_| UploadError::InvalidImage)?;
    let (width, height) = image::ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|_| UploadError::InvalidImage)?
        .into_dimensions()
        .map_err(|_| UploadError::InvalidImage)?;
    if width > MAX_EDGE || height > MAX_EDGE || u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(UploadError::InvalidImage);
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_EDGE);
    limits.max_image_height = Some(MAX_EDGE);
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    reader.limits(limits);
    let raster = reader.decode().map_err(|_| UploadError::InvalidImage)?;
    let mut png = Cursor::new(Vec::new());
    raster
        .write_to(&mut png, image::ImageFormat::Png)
        .map_err(|_| UploadError::InvalidImage)?;
    preflight("pasted-image.png", "image/png", png.into_inner().into())
}

fn preflight(name: &str, mime: &str, bytes: Arc<[u8]>) -> Intake {
    let declaration = ImageDeclaration::new(name, mime, &bytes)?;
    Ok((declaration, bytes))
}

#[cfg(test)]
mod tests {
    use super::ingest;
    use subspace_api_client::uploads::UploadError;
    use subspace_framework::dashboard::chat_input::attachments::AttachmentSource;

    #[test]
    fn intake_rejects_a_directory_with_a_supported_file_extension() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.txt");
        std::fs::create_dir(&path).unwrap();
        assert!(matches!(
            ingest(AttachmentSource::Path(path)),
            Err(UploadError::InvalidImage)
        ));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn intake_rejects_a_fifo_without_opening_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.txt");
        assert!(
            std::process::Command::new("/usr/bin/mkfifo")
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        // Keep a writer available so regressing to File::open cannot hang the test.
        let unblock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        let (send, receive) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = send.send(ingest(AttachmentSource::Path(path)));
        });
        let result = receive.recv_timeout(std::time::Duration::from_secs(1));
        drop(unblock);
        assert!(matches!(result, Ok(Err(UploadError::InvalidImage))));
    }
}
