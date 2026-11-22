use gpui_kit::base::TestSupportExt as _;
use gpui_kit::{
    AnyElement, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, div,
};
use subspace_framework::dashboard::chart::Chart;

use super::ProjectView;
use crate::project::{Component, ResultValue};

impl ProjectView {
    pub(super) fn render_chart(&self, component: &Component) -> AnyElement {
        match self
            .snapshot
            .results()
            .get(&component.id)
            .map(|result| &result.value)
        {
            Some(ResultValue::Chart { chart }) => div()
                .id(self.element_id(component))
                .test_support()
                .role(gpui_kit::Role::Image)
                .aria_label(component.label.clone())
                .child(
                    Chart::new(self.element_id(component), chart.clone())
                        .label(component.label.clone()),
                )
                .into_any_element(),
            _ => div()
                .child(component.label.clone())
                .child("No data")
                .into_any_element(),
        }
    }
}
