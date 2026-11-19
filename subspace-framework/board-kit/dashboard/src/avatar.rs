//! Compact, theme-aware profile photos and initials for dashboard rows.

use gpui_kit::component::{
    Sizable as _, Size,
    avatar::{Avatar, AvatarGroup},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{RenderImage, SharedString, Styled as _, px};

use crate::Tokens;
use std::sync::Arc;

const AVATAR_SIZE_PX: f32 = 24.0;
/// A message sender's avatar: Kit's avatar sizes are pixel tiers, and 24 px
/// is too small and 48 px too large beside a name and a first line of text.
pub const SENDER_SIZE_PX: f32 = 32.0;

const GROUP_AVATAR_LIMIT: usize = 2;

pub fn person(name: impl Into<SharedString>, photo: Option<Arc<RenderImage>>) -> Avatar {
    let name = name.into();
    Avatar::new()
        .name(name)
        .small()
        .when_some(photo, |avatar, image| avatar.src(image))
}

/// A message sender's identity in a chat transcript: initials on the
/// theme-aware identity colour, as a rounded square rather than a circle.
/// Kit sizes a custom avatar's initials box but not their text, so the
/// text follows the secondary size.
pub fn sender(name: impl Into<SharedString>, tokens: &Tokens) -> Avatar {
    Avatar::new()
        .name(name)
        .with_size(Size::Size(px(SENDER_SIZE_PX)))
        .text_size(tokens.secondary_size())
        .rounded(tokens.surface_radius)
}

/// A person in a member list: Kit's smallest avatar, the same shape as
/// [`sender`].
pub fn member(name: impl Into<SharedString>, tokens: &Tokens) -> Avatar {
    Avatar::new()
        .name(name)
        .xsmall()
        .rounded(tokens.control_radius)
}

pub fn group(people: impl IntoIterator<Item = Avatar>) -> AvatarGroup {
    let people: Vec<_> = people.into_iter().take(GROUP_AVATAR_LIMIT).collect();
    AvatarGroup::new()
        .small()
        .limit(GROUP_AVATAR_LIMIT)
        .flex_shrink_0()
        .w(px(AVATAR_SIZE_PX * GROUP_AVATAR_LIMIT as f32))
        .children(people)
}
