//! The reserved `upgrade` root command (REQ-04). It runs before any GUI or
//! board configuration, so `--check` and an install both work headless.

use std::process::ExitCode;

use anyhow::{Context, Result};
use subspace_product::{Channel, UPGRADE_COMMAND, UPGRADE_USAGE, ValidatedProduct};
use usage::parse::{ParseOutput, ParseValue, Parser};

use crate::version::{is_newer, require_release_version};
use crate::{Installed, Release, Updater};

/// Exit code for a command line this command cannot act on.
const USAGE_EXIT: u8 = 2;

/// What the command line asked for.
struct Request {
    check: bool,
    version: Option<String>,
}

/// A command line that parsed, or the help page it asked for.
enum Parsed {
    Help(String),
    Request(Request),
}

/// A wrong command line, which exits 2, or a failed operation, which exits 1.
enum Failure {
    Usage(String),
    Operation(anyhow::Error),
}

/// Runs `upgrade`. `args` excludes the program name and starts with
/// `upgrade`.
pub fn run_cli(product: &ValidatedProduct, version: &str, args: &[String]) -> ExitCode {
    match upgrade(product, version, args) {
        Ok(report) => {
            println!("{report}");
            ExitCode::SUCCESS
        }
        Err(Failure::Usage(message)) => {
            eprintln!("{message}");
            ExitCode::from(USAGE_EXIT)
        }
        Err(Failure::Operation(error)) => {
            eprintln!("{error:#}");
            ExitCode::FAILURE
        }
    }
}

fn upgrade(product: &ValidatedProduct, version: &str, args: &[String]) -> Result<String, Failure> {
    match parse(product, args) {
        Err(message) => Err(Failure::Usage(message)),
        Ok(Parsed::Help(page)) => Ok(page),
        Ok(Parsed::Request(request)) => run(product, version, &request).map_err(Failure::Operation),
    }
}

fn parse(product: &ValidatedProduct, args: &[String]) -> Result<Parsed, String> {
    let spec = spec_for(product)?;
    let parsed = Parser::new(&spec)
        .explain(&argv_for(product, args))
        .map_err(|error| error.to_string())?;
    match parsed.errors.first() {
        Some(usage::error::UsageErr::Help(page)) => Ok(Parsed::Help(page.clone())),
        Some(error) => Err(error.to_string()),
        None => Ok(Parsed::Request(requested(&parsed, channel_of(product))?)),
    }
}

/// The channel the app updates within. An app without an update source
/// fails later with its own message.
fn channel_of(product: &ValidatedProduct) -> Channel {
    product
        .update
        .as_ref()
        .map_or(Channel::Stable, |source| source.channel)
}

fn spec_for(product: &ValidatedProduct) -> Result<usage::Spec, String> {
    format!("bin {:?}\n{UPGRADE_USAGE}\n", product.binary_name.as_str())
        .parse::<usage::Spec>()
        .map_err(|error| error.to_string())
}

fn argv_for(product: &ValidatedProduct, args: &[String]) -> Vec<String> {
    [product.binary_name.as_str(), UPGRADE_COMMAND]
        .into_iter()
        .map(str::to_owned)
        .chain(args.iter().skip(1).cloned())
        .collect()
}

fn requested(parsed: &ParseOutput, channel: Channel) -> Result<Request, String> {
    let check = matches!(flag(parsed, "check"), Some(ParseValue::Bool(true)));
    let version = match flag(parsed, "version") {
        Some(ParseValue::String(value)) => Some(
            require_release_version(channel, value)
                .map_err(|error| format!("--version: {error}"))?,
        ),
        _ => None,
    };
    Ok(Request { check, version })
}

fn flag<'a>(parsed: &'a ParseOutput, name: &str) -> Option<&'a ParseValue> {
    parsed
        .flags
        .iter()
        .find(|(flag, _)| flag.name == name)
        .map(|(_, value)| value)
}

fn run(product: &ValidatedProduct, version: &str, request: &Request) -> Result<String> {
    let updater = Updater::for_product(product, version).context(
        "this app has no update source, so it cannot update itself; install a release or use the package manager that installed it",
    )?;
    let release = select(&updater, request)?;
    if request.check {
        return Ok(check_report(version, &release));
    }
    if request.version.is_none() && !is_newer(channel_of(product), &release.version, version) {
        return Ok(up_to_date_report(product, version));
    }
    Ok(installed_report(&updater.install(&release)?))
}

fn select(updater: &Updater, request: &Request) -> Result<Release> {
    match request.version.as_deref() {
        Some(version) => updater.find(version),
        None => updater.latest(),
    }
}

fn check_report(version: &str, release: &Release) -> String {
    format!(
        "running {version}\nnewest {}\n{}",
        release.version, release.html_url
    )
}

fn up_to_date_report(product: &ValidatedProduct, version: &str) -> String {
    format!("{} {version} is up to date", product.binary_name.as_str())
}

fn installed_report(installed: &Installed) -> String {
    format!(
        "{} → {} installed. Quit and restart the app to use it.",
        installed.previous_version, installed.version
    )
}

#[cfg(test)]
mod tests {
    use std::process::ExitCode;

    use subspace_product::{ValidatedProduct, fixture};

    use super::{
        Parsed, USAGE_EXIT, check_report, installed_report, parse, run_cli, up_to_date_report,
    };
    use crate::{Installed, Release};

    fn product() -> ValidatedProduct {
        fixture::products()
            .expect("the known products validate")
            .into_iter()
            .find(|product| product.id.as_str() == "subspace")
            .expect("the product exists")
    }

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn no_flags_asks_for_the_newest_release() {
        let parsed = parse(&product(), &args(&["upgrade"])).expect("a bare upgrade parses");
        let Parsed::Request(request) = parsed else {
            panic!("a bare upgrade is not a help request")
        };
        assert!(!request.check);
        assert_eq!(request.version, None);
    }

    #[test]
    fn check_and_an_explicit_version_are_both_read() {
        let parsed = parse(
            &product(),
            &args(&["upgrade", "--check", "--version", "v1.2.0"]),
        )
        .expect("both flags parse");
        let Parsed::Request(request) = parsed else {
            panic!("flags are not a help request")
        };
        assert!(request.check);
        assert_eq!(request.version.as_deref(), Some("1.2.0"));
    }

    #[test]
    fn a_nightly_upgrade_refuses_a_stable_version_and_names_its_type() {
        let nightly = subspace_product::validate_product(
            &subspace_product::fixture::descriptor_for(subspace_product::Channel::Nightly),
        )
        .expect("the nightly descriptor validates");
        let Err(message) = parse(&nightly, &args(&["upgrade", "--version", "1.10.1"])) else {
            panic!("a stable version is not a nightly upgrade")
        };
        assert!(message.contains("only nightly versions"), "{message}");
        let pinned = "1.10.2-nightly.202611010200.ga43a333";
        let Ok(Parsed::Request(request)) =
            parse(&nightly, &args(&["upgrade", "--version", pinned]))
        else {
            panic!("a nightly version parses")
        };
        assert_eq!(request.version.as_deref(), Some(pinned));
    }

    #[test]
    fn help_is_a_page_rather_than_an_error() {
        let parsed = parse(&product(), &args(&["upgrade", "--help"])).expect("help parses");
        let Parsed::Help(page) = parsed else {
            panic!("--help must return a page")
        };
        assert!(page.contains("--check"), "{page}");
        assert!(page.contains("subspace"), "{page}");
    }

    #[test]
    fn a_wrong_command_line_is_a_usage_error() {
        for line in [
            args(&["upgrade", "--nonsense"]),
            args(&["upgrade", "--version"]),
            args(&["upgrade", "--version", "1.2"]),
            args(&["upgrade", "--version", "latest"]),
        ] {
            assert!(parse(&product(), &line).is_err(), "{line:?}");
        }
    }

    /// `ExitCode` has no equality, so compare the debug forms.
    fn code(value: ExitCode) -> String {
        format!("{value:?}")
    }

    #[test]
    fn help_exits_zero_and_a_wrong_command_line_exits_two() {
        let product = product();
        assert_eq!(
            code(run_cli(&product, "1.2.0", &args(&["upgrade", "--help"]))),
            code(ExitCode::SUCCESS)
        );
        assert_eq!(
            code(run_cli(
                &product,
                "1.2.0",
                &args(&["upgrade", "--nonsense"])
            )),
            code(ExitCode::from(USAGE_EXIT))
        );
    }

    #[test]
    fn an_app_without_an_update_source_reports_why_it_cannot_update() {
        let mut product = product();
        product.update = None;
        assert_eq!(
            code(run_cli(&product, "1.2.0", &args(&["upgrade", "--check"]))),
            code(ExitCode::FAILURE)
        );
    }

    #[test]
    fn the_reports_name_the_versions_and_the_restart_rule() {
        let release = Release {
            tag: "subspace-desktop-v1.2.0".into(),
            version: "1.2.0".into(),
            html_url: "https://example.test/release".into(),
            assets: Default::default(),
        };
        assert_eq!(
            check_report("1.1.0", &release),
            "running 1.1.0\nnewest 1.2.0\nhttps://example.test/release"
        );
        assert_eq!(
            up_to_date_report(&product(), "1.2.0"),
            "subspace 1.2.0 is up to date"
        );
        let installed = Installed {
            previous_version: "1.1.0".into(),
            version: "1.2.0".into(),
            path: std::path::PathBuf::from("/opt/subspace"),
        };
        let report = installed_report(&installed);
        assert!(report.starts_with("1.1.0 → 1.2.0 installed."), "{report}");
        assert!(
            report.ends_with("Quit and restart the app to use it."),
            "{report}"
        );
    }
}
