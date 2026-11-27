//! Settings › TSX Spaces: the trusted signing roots, each installed
//! package as Settings shows it, and one grant per capability a package
//! declares. Grants follow the Settings Save boundary like a web Space's
//! permissions. Settings builds the page on the UI thread, so it reads the
//! inventory the Space writes after verifying packages off it, and never
//! verifies a package itself.

use std::path::Path;

use serde_json::{Value, json};
use subspace_framework::dashboard::settings::{BoardFields, FieldKind, FieldSpec, PermissionSpec};
use subspace_tsx_system::Tier;

use crate::grant::Identity;
use crate::saved;
use crate::store::{Installed, Paths};

/// What a capability lets a package do, in plain words.
pub(crate) fn plain_words(tier: Tier) -> &'static str {
    match tier {
        Tier::Metrics => "read CPU, memory and network use",
        Tier::Apps => "read which apps use the most memory, by name",
        Tier::Facts => "read facts about this computer, such as its OS, CPU and displays",
    }
}

/// One installed package as Settings shows it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Listing {
    file: String,
    package: Result<Summary, String>,
}

#[derive(Clone, Debug, PartialEq)]
struct Summary {
    name: String,
    version: String,
    identity: Identity,
}

/// The Settings page for the config file at `config_file`.
pub fn settings_fields(config_file: &Path) -> BoardFields {
    describe(&read_inventory(&Paths::beside(config_file)))
}

/// Records what the latest verification found, for Settings: staged and
/// renamed, so Settings never reads half a file. A failure to write leaves
/// the last inventory: Settings then lists what it had.
pub(crate) fn write_inventory(paths: &Paths, packages: &[Installed]) {
    let entries: Vec<Value> = packages
        .iter()
        .map(|package| to_json(&listing(package)))
        .collect();
    let _ = crate::store::replace(
        &paths.inventory(),
        Value::Array(entries).to_string().as_bytes(),
    );
}

fn read_inventory(paths: &Paths) -> Vec<Listing> {
    std::fs::read_to_string(paths.inventory())
        .ok()
        .and_then(|text| serde_json::from_str::<Vec<Value>>(&text).ok())
        .map(|entries| entries.iter().filter_map(from_json).collect())
        .unwrap_or_default()
}

fn listing(package: &Installed) -> Listing {
    Listing {
        file: package.file.display().to_string(),
        package: package
            .state
            .as_ref()
            .map_err(Clone::clone)
            .map(|verified| Summary {
                name: verified.name.clone(),
                version: verified.version.clone(),
                identity: verified.identity.clone(),
            }),
    }
}

fn to_json(listing: &Listing) -> Value {
    match &listing.package {
        Ok(summary) => json!({
            "file": listing.file,
            "name": summary.name,
            "version": summary.version,
            "appId": summary.identity.app_id,
            "keyId": summary.identity.key_id,
            "install": summary.identity.install,
            "declared": summary.identity.declared.iter().map(|tier| tier.host_name()).collect::<Vec<_>>(),
        }),
        Err(reason) => json!({ "file": listing.file, "error": reason }),
    }
}

fn from_json(entry: &Value) -> Option<Listing> {
    let text = |key: &str| entry[key].as_str().map(str::to_owned);
    let file = text("file")?;
    if let Some(reason) = text("error") {
        return Some(Listing {
            file,
            package: Err(reason),
        });
    }
    let declared: Vec<String> = entry["declared"]
        .as_array()?
        .iter()
        .filter_map(|name| name.as_str().map(str::to_owned))
        .collect();
    Some(Listing {
        file,
        package: Ok(Summary {
            name: text("name")?,
            version: text("version")?,
            identity: Identity {
                app_id: text("appId")?,
                key_id: text("keyId")?,
                install: text("install")?,
                declared: Identity::declared_tiers(&declared),
            },
        }),
    })
}

fn describe(packages: &[Listing]) -> BoardFields {
    let roots = FieldSpec::new(
        saved::TRUSTED_ROOTS.into(),
        "Trusted signing roots".into(),
        Some("Root public keys, in base64, separated by commas. Only packages signed under one of them install and run. None is trusted by default.".into()),
        FieldKind::Text { default: None },
    );
    BoardFields::new(
        vec![saved::SECTION.into()],
        vec![roots],
        notes(packages),
        permissions(packages),
    )
}

fn notes(packages: &[Listing]) -> Vec<String> {
    if packages.is_empty() {
        return vec![
            "No TSX Spaces are listed. Install a package, or open TSX Spaces to check the installed ones.".into(),
        ];
    }
    packages.iter().map(note).collect()
}

fn note(listing: &Listing) -> String {
    match &listing.package {
        Ok(summary) => {
            let asks: Vec<&str> = summary
                .identity
                .declared
                .iter()
                .map(|tier| plain_words(*tier))
                .collect();
            let asks = if asks.is_empty() {
                "It asks for no capabilities.".to_owned()
            } else {
                format!(
                    "It asks to {}. Each stays off until you allow it below and save.",
                    asks.join("; ")
                )
            };
            format!(
                "{} {} ({}), signed with release key {}. {asks}",
                summary.name, summary.version, summary.identity.app_id, summary.identity.key_id
            )
        }
        Err(reason) => format!("{} cannot run: {reason}", listing.file),
    }
}

fn permissions(packages: &[Listing]) -> Vec<PermissionSpec> {
    packages
        .iter()
        .filter_map(|listing| listing.package.as_ref().ok())
        .flat_map(|summary| {
            summary.identity.declared.iter().map(|tier| {
                PermissionSpec::new(
                    summary.identity.grant_key(*tier),
                    format!("Allow {} to {}", summary.name, plain_words(*tier)),
                )
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{describe, read_inventory, settings_fields, write_inventory};
    use crate::fixtures::{package, signer};
    use crate::store::{Paths, install, installed};

    #[test]
    fn with_nothing_listed_settings_offers_the_roots_and_says_so() {
        let fields = describe(&[]);
        assert_eq!(fields.section, ["tsxSpaces"]);
        assert_eq!(fields.fields.len(), 1);
        assert_eq!(fields.fields[0].key, "trustedRoots");
        assert!(fields.permissions.is_empty());
        assert!(fields.notes[0].contains("No TSX Spaces are listed"));
    }

    #[test]
    fn settings_reads_the_inventory_the_space_wrote_without_verifying() {
        let folder = tempfile::tempdir().unwrap();
        let config = folder.path().join("config.json");
        let paths = Paths::beside(&config);
        let signer = signer();
        let roots = [signer.root.clone()];
        let source = folder.path().join("app.nshpkg");
        std::fs::write(&source, package(&signer, "dev.example.app", 1, false)).unwrap();
        let verified = install(&paths, &roots, &source).unwrap();
        write_inventory(&paths, &installed(&paths, &roots));
        assert_eq!(read_inventory(&paths).len(), 1);
        // The package is gone, so only the inventory can still list it.
        std::fs::remove_dir_all(folder.path().join("tsx").join("packages")).unwrap();
        let fields = settings_fields(&config);
        assert_eq!(
            fields.permissions[0].key,
            verified
                .identity
                .grant_key(subspace_tsx_system::Tier::Metrics)
        );
    }
}
