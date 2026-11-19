//! About consumes product attribution without applying Markdown or copy edits.

use super::presentation::{build_channel, credits, version_line};
use gpui_kit::TestAppContext;
use subspace_product::{Channel, ProductDescriptor, fixture, validate_product};

const CREDIT_LINES: [&str; 2] = [
    "Created by Example Studio & Co.",
    "Assisted by <fixtures> and Rust.",
];

fn credited_descriptor() -> ProductDescriptor {
    let mut descriptor = fixture::descriptor();
    descriptor.credits = CREDIT_LINES.map(str::to_owned).to_vec();
    descriptor
}

#[gpui_kit::test]
fn credits_are_product_metadata_and_absent_products_have_no_section(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        let mut descriptor = credited_descriptor();
        let product = validate_product(&descriptor).unwrap();
        assert_eq!(product.credits, CREDIT_LINES);
        assert!(credits(&product, cx).is_some());
        descriptor.credits.clear();
        assert!(credits(&validate_product(&descriptor).unwrap(), cx).is_none());
    });
}

#[test]
fn build_channel_identifies_local_release_and_nightly_builds() {
    assert_eq!(build_channel("dev", Channel::Stable), "Development");
    assert_eq!(build_channel("release", Channel::Stable), "Release");
    assert_eq!(build_channel("release", Channel::Nightly), "Nightly");
}

struct CreditFixture(subspace_product::ValidatedProduct);

impl gpui_kit::Render for CreditFixture {
    fn render(
        &mut self,
        _: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<Self>,
    ) -> impl gpui_kit::IntoElement {
        use gpui_kit::ParentElement as _;
        gpui_kit::component::v_flex().children(credits(&self.0, cx))
    }
}

#[test]
fn the_version_line_drops_the_product_name_shown_above_it() {
    let text = "Subspace v0.6.0-dev\n\nRepository: https://github.com/example/repo";
    assert_eq!(version_line(text, "Subspace"), "v0.6.0-dev");
    assert_eq!(
        version_line("Example Subspace v0.6.0", "Example Subspace"),
        "v0.6.0"
    );
    assert_eq!(version_line("v1.2.3", "Subspace"), "v1.2.3");
    assert_eq!(version_line("", "Subspace"), "");
}

#[gpui_kit::test]
fn the_rendered_credits_are_verbatim_and_omit_an_empty_product(cx: &mut TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;
    cx.update(gpui_kit::init);
    let product = validate_product(&credited_descriptor()).unwrap();
    let (view, cx) = cx.add_window_view(|_, _| CreditFixture(product));
    cx.update(|window, cx| {
        window.replace_root(cx, |window, cx| {
            gpui_kit::base::Root::new(view.clone(), window, cx)
        });
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.render_frame(cx);
        let section = window.find("about-credits-section").bounds();
        // The section's one-rem top inset precedes the selectable text.
        let start = gpui_kit::point(
            section.left() + gpui_kit::px(1.),
            section.top() + window.rem_size() + gpui_kit::px(2.),
        );
        let end = gpui_kit::point(
            section.right() - gpui_kit::px(1.),
            section.bottom() - gpui_kit::px(1.),
        );
        window.drag(start, end, cx);
        assert_eq!(
            gpui_kit::base::TextSelection::selected_text(window, cx),
            CREDIT_LINES.join("\n")
        );
    });
    view.update(cx, |view, cx| {
        view.0.credits.clear();
        cx.notify();
    });
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("about-credits-section").is_none());
    });
}
