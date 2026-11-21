use super::{Dock, Docks, Room, fit, offered, pane_width, saved_value, shown, toggled};

const BOTH: Docks = Docks {
    navigator: true,
    inspector: true,
};
const NAVIGATOR_ONLY: Docks = Docks {
    navigator: true,
    inspector: false,
};
const NEITHER: Docks = Docks {
    navigator: false,
    inspector: false,
};

/// The default docks in a window `width` wide.
fn room(width: f32) -> Room {
    Room {
        width,
        navigator: 240.,
        inspector: 320.,
    }
}

#[test]
fn both_docks_show_until_they_are_hidden() {
    assert_eq!(Docks::default(), BOTH);
    assert!(shown(None), "nothing saved yet");
    assert!(shown(Some("shown")));
    assert!(shown(Some("anything else")), "an unknown value shows");
    assert!(!shown(Some("hidden")));
    for choice in [true, false] {
        assert_eq!(shown(Some(saved_value(choice))), choice);
    }
}

#[test]
fn a_narrow_window_holds_the_inspector_back_first_then_the_navigator() {
    assert_eq!(fit(BOTH, room(1440.)), BOTH);
    assert_eq!(fit(BOTH, room(1000.)), BOTH, "the pane keeps 440 px");
    assert_eq!(fit(BOTH, room(999.)), NAVIGATOR_ONLY);
    assert_eq!(fit(BOTH, room(680.)), NAVIGATOR_ONLY);
    assert_eq!(fit(BOTH, room(679.)), NEITHER);
    assert_eq!(fit(BOTH, room(0.)), BOTH, "before the first render");
}

#[test]
fn a_hidden_dock_stays_hidden_and_leaves_its_room_to_the_other() {
    let inspector_only = Docks {
        navigator: false,
        inspector: true,
    };
    assert_eq!(fit(NEITHER, room(1440.)), NEITHER);
    assert_eq!(fit(NAVIGATOR_ONLY, room(1440.)), NAVIGATOR_ONLY);
    assert_eq!(fit(inspector_only, room(760.)), inspector_only);
    assert_eq!(fit(inspector_only, room(759.)), NEITHER);
}

#[test]
fn a_view_without_an_inspector_keeps_the_choice_but_shows_none() {
    assert_eq!(offered(BOTH, true), BOTH);
    assert_eq!(offered(BOTH, false), NAVIGATOR_ONLY, "the activity feed");
    assert_eq!(offered(NEITHER, true), NEITHER, "hidden stays hidden");
    assert_eq!(
        fit(offered(BOTH, false), room(800.)),
        NAVIGATOR_ONLY,
        "no inspector to make room for, so the navigator stays"
    );
}

#[test]
fn the_pane_takes_what_the_shown_docks_leave() {
    assert_eq!(pane_width(BOTH, room(1440.)), 880.);
    assert_eq!(pane_width(NAVIGATOR_ONLY, room(1440.)), 1200.);
    assert_eq!(pane_width(NEITHER, room(700.)), 700.);
    assert_eq!(pane_width(BOTH, room(0.)), 0., "never below nothing");
}

#[test]
fn a_switch_hides_what_shows_and_chooses_what_is_hidden() {
    assert_eq!(
        toggled(BOTH, BOTH, Dock::Inspector),
        Some(NAVIGATOR_ONLY),
        "Close hides the inspector"
    );
    assert_eq!(
        toggled(NAVIGATOR_ONLY, NAVIGATOR_ONLY, Dock::Inspector),
        Some(BOTH)
    );
    assert_eq!(
        toggled(BOTH, BOTH, Dock::Navigator),
        Some(Docks {
            navigator: false,
            inspector: true,
        })
    );
    assert_eq!(
        toggled(BOTH, NAVIGATOR_ONLY, Dock::Inspector),
        None,
        "chosen already: only the window holds it back"
    );
    assert_eq!(
        toggled(BOTH, NAVIGATOR_ONLY, Dock::Navigator),
        Some(Docks {
            navigator: false,
            inspector: true,
        }),
        "a held-back inspector does not stop the navigator's switch"
    );
}
