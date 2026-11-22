use super::Group;

pub(super) fn needs_approval(group: Group, action: &str) -> bool {
    !matches!(group, Group::Inspect)
        && !matches!(action, "add_control" | "parameters" | "import_canvas")
}

#[cfg(test)]
mod tests {
    use super::Group;
    use super::needs_approval;

    #[test]
    fn inspect_never_needs_approval() {
        assert!(!needs_approval(Group::Inspect, "draft"));
        assert!(!needs_approval(Group::Inspect, "environments"));
    }

    #[test]
    fn local_edit_helpers_skip_approval() {
        assert!(!needs_approval(Group::Edit, "add_control"));
        assert!(!needs_approval(Group::Edit, "parameters"));
        assert!(!needs_approval(Group::Edit, "import_canvas"));
        assert!(needs_approval(Group::Edit, "save"));
        assert!(needs_approval(Group::Run, "execute"));
        assert!(needs_approval(Group::Environment, "provision"));
    }
}
