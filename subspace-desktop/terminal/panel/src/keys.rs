use gpui_kit::actions;
use subspace_framework::dashboard::KeyHelp;

actions!(
    terminal,
    [
        NewTab,
        CloseTab,
        NextTab,
        PreviousTab,
        Restart,
        Retry,
        ConfirmClose,
        CancelClose
    ]
);

pub const KEYS: &[KeyHelp] = &[
    KeyHelp::new("Cmd/Ctrl+T", "New terminal tab"),
    KeyHelp::new("Cmd/Ctrl+W", "Close terminal tab"),
    KeyHelp::new("Cmd/Ctrl+>", "Next terminal tab"),
    KeyHelp::new("Cmd/Ctrl+<", "Previous terminal tab"),
];
