use gpui_kit::actions;

pub(super) use crate::app::shortcuts::matching_keys;

actions!(subspace_help, [FocusShortcutFilter]);

#[cfg(test)]
mod tests {
    use super::matching_keys;

    #[test]
    fn filters_keys_and_meanings_by_every_word_ignoring_case() {
        let keys = vec![
            ("w".into(), "Wrap long titles".into()),
            ("r".into(), "Reload".into()),
        ];
        assert_eq!(matching_keys(&keys, "LONG wrap").len(), 1);
        assert_eq!(matching_keys(&keys, "W")[0].0.as_ref(), "w");
        assert!(matching_keys(&keys, "wrap reload").is_empty());
        assert_eq!(matching_keys(&keys, "  "), keys);
    }
}
