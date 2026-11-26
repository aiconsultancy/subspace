use super::composition::root_context;
use super::strings;
use crate::{Composition, root, tree::Capabilities};
use std::process::ExitCode;
use subspace_product::ProductContext;

fn fallback(_: &ProductContext, _: &[String]) -> ExitCode {
    ExitCode::from(7)
}

fn separated_tree(context: &ProductContext) -> Result<usage::Spec, String> {
    format!(
        "bin {:?}\narg \"[words]...\" double_dash=\"required\"",
        context.product.binary_name.as_str()
    )
    .parse::<usage::Spec>()
    .map_err(|error| error.to_string())
}

#[test]
fn a_declared_root_argument_refusing_a_word_is_an_error_not_an_unmatched_command() {
    let composition = Composition::new(separated_tree, root::complete_word, fallback);
    for (args, code) in [(&["external"][..], 2), (&["--", "external"], 7)] {
        assert_eq!(
            composition.run_catalog(&root_context(), &Capabilities::default(), &strings(args)),
            ExitCode::from(code),
            "{args:?}"
        );
    }
}

#[cfg(unix)]
static MOUNT_LOG: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

#[cfg(unix)]
fn mounted_tree(context: &ProductContext) -> Result<usage::Spec, String> {
    let log = MOUNT_LOG.get().ok_or("no mount log")?.display().to_string();
    let run = format!("echo run >> '{log}'; echo 'cmd \"build\"'");
    format!(
        "bin {:?}\ncmd \"tasks\" {{\n mount run={run:?}\n}}",
        context.product.binary_name.as_str()
    )
    .parse::<usage::Spec>()
    .map_err(|error| error.to_string())
}

#[cfg(unix)]
#[test]
fn a_refused_line_runs_mounted_command_discovery_once() {
    let folder = tempfile::tempdir().unwrap();
    let log = MOUNT_LOG.get_or_init(|| folder.path().join("mount.log"));
    let composition = Composition::new(mounted_tree, root::complete_word, fallback);
    assert_eq!(
        composition.run_catalog(
            &root_context(),
            &Capabilities::default(),
            &strings(&["tasks", "build", "bogus"])
        ),
        ExitCode::from(2)
    );
    assert_eq!(std::fs::read_to_string(log).unwrap(), "run\n");
    assert_eq!(
        composition.run_catalog(
            &root_context(),
            &Capabilities::default(),
            &strings(&["external"])
        ),
        ExitCode::from(7)
    );
    assert_eq!(std::fs::read_to_string(log).unwrap(), "run\n");
}

#[cfg(unix)]
fn root_mount_tree(context: &ProductContext) -> Result<usage::Spec, String> {
    format!(
        "bin {:?}\nmount run=\"echo 'cmd \\\"build\\\"'\"",
        context.product.binary_name.as_str()
    )
    .parse::<usage::Spec>()
    .map_err(|error| error.to_string())
}

#[cfg(unix)]
#[test]
fn a_root_mount_owns_the_words_it_does_not_declare() {
    let composition = Composition::new(root_mount_tree, root::complete_word, fallback);
    for (args, code) in [(&["build"][..], 7), (&["external"], 2)] {
        assert_eq!(
            composition.run_catalog(&root_context(), &Capabilities::default(), &strings(args)),
            ExitCode::from(code),
            "{args:?}"
        );
    }
}

fn file_tree(context: &ProductContext) -> Result<usage::Spec, String> {
    format!(
        "bin {:?}\narg \"<file>\" double_dash=\"required\"",
        context.product.binary_name.as_str()
    )
    .parse::<usage::Spec>()
    .map_err(|error| error.to_string())
}

#[test]
fn a_stop_after_a_root_argument_error_or_binding_is_not_an_unmatched_command() {
    let composition = Composition::new(file_tree, root::complete_word, fallback);
    for args in [
        &["stray", "--", "valid.txt", "extra"][..],
        &["--", "valid.txt", "extra"],
    ] {
        assert_eq!(
            composition.run_catalog(&root_context(), &Capabilities::default(), &strings(args)),
            ExitCode::from(2),
            "{args:?}"
        );
    }
}

#[cfg(unix)]
fn scripted_choices_tree(context: &ProductContext) -> Result<usage::Spec, String> {
    format!(
        "bin {:?}\nflag \"--env <ENV>\" {{\n choices run=\"echo staging\"\n}}",
        context.product.binary_name.as_str()
    )
    .parse::<usage::Spec>()
    .map_err(|error| error.to_string())
}

#[cfg(unix)]
#[test]
fn a_scripted_choice_the_script_refuses_is_an_error_not_an_unmatched_command() {
    let composition = Composition::new(scripted_choices_tree, root::complete_word, fallback);
    for (args, code) in [
        (&["--env", "production", "external"][..], 2),
        (&["--env", "staging", "external"], 7),
    ] {
        assert_eq!(
            composition.run_catalog(&root_context(), &Capabilities::default(), &strings(args)),
            ExitCode::from(code),
            "{args:?}"
        );
    }
}

fn dry_run_tree(context: &ProductContext) -> Result<usage::Spec, String> {
    format!(
        "bin {:?}\nflag \"--dry-run\" bool_value=#true\nflag \"--mode <MODE>\" required=#false {{\n choices \"fast\" \"slow\"\n}}",
        context.product.binary_name.as_str()
    )
    .parse::<usage::Spec>()
    .map_err(|error| error.to_string())
}

#[test]
fn an_invalid_root_flag_before_an_unmatched_word_is_reported() {
    let composition = Composition::new(dry_run_tree, root::complete_word, fallback);
    for (args, code) in [
        (&["--dry-run=bogus", "external"][..], 2),
        (&["--mode", "bogus", "external"], 2),
        (&["--mode"], 2),
        (&["--", "external"], 2),
        (&["--dry-run", "--", "external"], 2),
        (&["--dry-run=true", "external"], 7),
        (&["--dry-run", "--mode", "fast", "external", "raw"], 7),
    ] {
        assert_eq!(
            composition.run_catalog(&root_context(), &Capabilities::default(), &strings(args)),
            ExitCode::from(code),
            "{args:?}"
        );
    }
}
