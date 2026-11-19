//! Adapt the read model to the board's Subspace API Markdown pre-processor.

use std::collections::HashMap;

use crate::markdown::{Media, MentionStatus, Options, prepare_markdown};
use gpui_kit::SharedString;
use subspace_api_client::model::{
    IssueActivity, IssueDetail, IssueMedia, MentionStatus as ApiMention,
};

pub(super) fn description(detail: &IssueDetail, media: &[IssueMedia]) -> Option<SharedString> {
    let source = detail
        .description
        .as_deref()
        .filter(|text| !text.trim().is_empty())?;
    Some(prepare(source, media, &[]).into())
}

pub(super) fn comments(
    items: &[IssueActivity],
    projected: &[subspace_api_client::model::Comment],
) -> HashMap<String, SharedString> {
    items
        .iter()
        .filter_map(|entry| entry.comment.as_ref())
        .chain(projected)
        .filter(|comment| comment.deleted_at.is_none())
        .map(|comment| {
            (
                comment.id.clone(),
                prepare(&comment.body, &comment.media, &comment.mentions).into(),
            )
        })
        .collect()
}

fn prepare(source: &str, media: &[IssueMedia], mentions: &[ApiMention]) -> String {
    let media: Vec<_> = media
        .iter()
        .map(|media| {
            Media::new(
                &media.media_id,
                &media.file_name,
                &media.content_type,
                u64::try_from(media.size_bytes).unwrap_or_default(),
            )
        })
        .collect();
    let mentions: Vec<_> = mentions
        .iter()
        .filter_map(|mention| {
            Some(MentionStatus::new(
                &mention.user_id,
                mention.display_name.as_deref()?,
            ))
        })
        .collect();
    prepare_markdown(source, &Options::new(&media, &mentions))
}
