//! Explicit projections keep the composer controls available in a single row.

use super::{ChatToolbar, labels};
use crate::anchored::Anchor;
use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::{Context, Entity, IntoElement as _, Pixels, Window, div};
use subspace_dashboard::overflow_toolbar::{OverflowUnit, PopupEntry};
use subspace_lucide::IconName;

#[derive(Clone, Copy)]
pub(super) enum Control {
    Context,
    Mode,
    Model,
    Reasoning,
    Agent,
    View,
    Usage,
}

impl Control {
    pub(super) fn presentation(self) -> (&'static str, IconName, &'static str) {
        match self {
            Self::Context => ("chat-add", IconName::Plus, "Add context"),
            Self::Mode => ("chat-mode", IconName::Settings2, "Mode"),
            Self::Model => ("chat-model", IconName::Cpu, "Model"),
            Self::Reasoning => ("chat-reasoning", IconName::Settings2, "Reasoning"),
            Self::Agent => ("chat-agent", IconName::Bot, "Agent"),
            Self::View => ("chat-view", IconName::SlidersHorizontal, "View"),
            Self::Usage => ("chat-usage", IconName::Gauge, "Usage"),
        }
    }
}

impl ChatToolbar {
    pub(super) fn overflow_units(&self, cx: &Context<Self>) -> Vec<OverflowUnit> {
        let mut controls = vec![Control::Context];
        if self.model.copilot_controls {
            controls.push(Control::Mode);
        }
        controls.push(Control::Model);
        if !labels::effort_options(labels::current_model(
            &self.model.models,
            self.model.model.as_deref(),
        ))
        .is_empty()
        {
            controls.push(Control::Reasoning);
        }
        if self.model.copilot_controls {
            controls.push(Control::Agent);
        }
        controls.extend([Control::View, Control::Usage]);
        controls
            .into_iter()
            .map(|control| Self::unit(control, cx))
            .collect()
    }

    fn unit(control: Control, cx: &Context<Self>) -> OverflowUnit {
        let owner = cx.weak_entity();
        let popup_owner = owner.clone();
        let (key, icon, label) = control.presentation();
        let entry = PopupEntry::anchored(move |anchor| {
            let owner = popup_owner.clone();
            PopupMenuItem::new(label)
                .icon(icon)
                .on_click(move |_, window, cx| {
                    _ = owner.update(cx, |toolbar, cx| {
                        toolbar.open_control(control, anchor.host(), window, cx);
                    });
                })
        });
        OverflowUnit::inline(
            key,
            f32::from(Pixels::ZERO),
            move |_, cx| {
                owner
                    .update(cx, |toolbar, cx| match control {
                        Control::Model => toolbar.model_list(cx).into_any_element(),
                        Control::Usage => toolbar.usage(cx).into_any_element(),
                        _ => toolbar.menu_trigger(control, cx).into_any_element(),
                    })
                    .unwrap_or_else(|_| div().into_any_element())
            },
            vec![entry],
        )
    }

    fn open_control(
        &mut self,
        control: Control,
        host: Entity<Anchor>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dismiss_popups(window, cx);
        match control {
            Control::Model => self.show_models(host, window, cx),
            Control::Usage => Self::show_usage(host, window, cx),
            _ => {
                let contents = super::native_menu::NativeMenu::new(
                    control,
                    self.model.clone(),
                    cx.weak_entity(),
                );
                host.update(cx, |host, cx| {
                    host.show_menu_with(window, cx, |menu, window, cx| {
                        contents.build(menu, window, cx)
                    });
                });
            }
        }
    }
}
