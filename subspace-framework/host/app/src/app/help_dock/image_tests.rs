use gpui_kit::TestAppContext;
use subspace_agent_tools::BoardDoc;

use crate::app::help_dock::fixtures::{GUIDE, IMAGES};

#[gpui_kit::test]
fn a_host_owned_bundled_raster_reaches_the_native_help_image_cache(cx: &mut TestAppContext) {
    super::test_support::check_help_image_cache(
        BoardDoc {
            id: "fixture",
            title: "Fixture",
            markdown: GUIDE,
            images: IMAGES,
        },
        cx,
    )
    .unwrap();
    assert_eq!(IMAGES.len(), 3);
}
