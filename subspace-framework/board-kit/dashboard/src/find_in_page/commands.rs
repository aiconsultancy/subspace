//! The palette exposes navigation only while the active Space has an open Find.
use super::{Close, Next, Open, Previous};
use crate::PaletteCommand;
pub(crate) fn commands(open: bool) -> Vec<PaletteCommand> {
    let keys = shortcuts();
    let mut commands = vec![
        PaletteCommand::new("Find in this Space", Open)
            .keywords("find local text page")
            .shortcut(keys.open),
    ];
    if open {
        commands.extend([
            PaletteCommand::new("Next find match", Next).shortcut(keys.next),
            PaletteCommand::new("Previous find match", Previous).shortcut(keys.previous),
            PaletteCommand::new("Close find", Close).shortcut("Esc"),
        ]);
    }
    commands
}
pub struct Shortcuts {
    pub open: &'static str,
    pub next: &'static str,
    pub previous: &'static str,
}
pub fn shortcuts() -> Shortcuts {
    let (open, next, previous) = if cfg!(target_os = "macos") {
        ("⌘F", "⌘G", "⌘⇧G")
    } else {
        ("Ctrl+F", "Ctrl+G", "Ctrl+Shift+G")
    };
    Shortcuts {
        open,
        next,
        previous,
    }
}

#[cfg(test)]
mod tests {
    use super::commands;
    #[test]
    fn palette_find_navigation_follows_the_open_state() {
        assert_eq!(commands(false).len(), 1);
        let commands = commands(true);
        assert_eq!(
            commands
                .iter()
                .map(|c| c.label.as_ref())
                .collect::<Vec<_>>(),
            [
                "Find in this Space",
                "Next find match",
                "Previous find match",
                "Close find"
            ]
        );
        assert!(commands.iter().all(|c| c.shortcut.is_some()));
    }
}
