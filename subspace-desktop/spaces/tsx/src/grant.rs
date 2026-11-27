//! Capability grants. A grant is bound to the package's app ID, the release
//! key that signed it, the set of tiers it declares and the install: a build
//! signed by another key, one that declares more, or a package installed
//! again after Remove asks again; an update with the same key and set keeps
//! its grants.

use sha2::{Digest, Sha256};
use subspace_tsx_system::Tier;

use crate::saved::Saved;

/// Who a grant is for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Identity {
    pub(crate) app_id: String,
    /// The release key's ID.
    pub(crate) key_id: String,
    /// The install the grant belongs to: random, kept while updates keep the
    /// key, new after Remove.
    pub(crate) install: String,
    /// The tiers the package declares, in [`Tier::ALL`] order.
    pub(crate) declared: Vec<Tier>,
}

/// Hex digits of the binding kept in a grant key.
const BINDING_DIGITS: usize = 16;

impl Identity {
    /// The tiers among the host names a package declares. Names this host
    /// does not offer are left out: they are never granted.
    pub(crate) fn declared_tiers(names: &[String]) -> Vec<Tier> {
        Tier::ALL
            .into_iter()
            .filter(|tier| names.iter().any(|name| name == tier.host_name()))
            .collect()
    }

    /// The Settings key of the grant of `tier`.
    pub(crate) fn grant_key(&self, tier: Tier) -> String {
        format!("{}.{}", self.binding(), tier.host_name())
    }

    /// The tiers granted: declared, and switched on in Settings.
    pub(crate) fn granted(&self, saved: &Saved) -> Vec<Tier> {
        self.declared
            .iter()
            .copied()
            .filter(|tier| saved.allows(&self.grant_key(*tier)))
            .collect()
    }

    fn binding(&self) -> String {
        let declared: Vec<&str> = self.declared.iter().map(|tier| tier.host_name()).collect();
        let text = format!(
            "{}\0{}\0{}\0{}",
            self.app_id,
            self.key_id,
            declared.join(","),
            self.install
        );
        let digest = hex::encode(Sha256::digest(text.as_bytes()));
        format!("{}.{}", self.app_id, &digest[..BINDING_DIGITS])
    }
}

#[cfg(test)]
mod tests {
    use super::Identity;
    use crate::saved::Saved;
    use serde_json::{Map, Value};
    use subspace_tsx_system::Tier;

    fn monitor(key: &str, declared: Vec<Tier>) -> Identity {
        Identity {
            app_id: "dev.example.monitor".into(),
            key_id: key.into(),
            install: "install-a".into(),
            declared,
        }
    }

    fn saving(keys: &[String]) -> Saved {
        let permissions: Map<String, Value> = keys
            .iter()
            .map(|key| (key.clone(), Value::Bool(true)))
            .collect();
        Saved {
            permissions,
            ..Saved::default()
        }
    }

    #[test]
    fn declared_tiers_ignore_names_the_host_does_not_offer() {
        let names = vec![
            "subspace.system.apps".to_owned(),
            "subspace.system.shell".to_owned(),
            "subspace.system.metrics".to_owned(),
        ];
        assert_eq!(
            Identity::declared_tiers(&names),
            [Tier::Metrics, Tier::Apps]
        );
    }

    #[test]
    fn nothing_is_granted_until_settings_switches_it_on() {
        let identity = monitor("key-a", Tier::ALL.to_vec());
        assert!(identity.granted(&Saved::default()).is_empty());
        let saved = saving(&[identity.grant_key(Tier::Metrics)]);
        assert_eq!(identity.granted(&saved), [Tier::Metrics]);
    }

    #[test]
    fn another_key_or_a_larger_declaration_asks_again() {
        let first = monitor("key-a", vec![Tier::Metrics]);
        let saved = saving(&[first.grant_key(Tier::Metrics)]);
        assert!(
            monitor("key-b", vec![Tier::Metrics])
                .granted(&saved)
                .is_empty()
        );
        let larger = monitor("key-a", vec![Tier::Metrics, Tier::Facts]);
        assert!(larger.granted(&saved).is_empty());
    }

    #[test]
    fn an_update_with_the_same_key_and_tiers_keeps_its_grants() {
        let saved = saving(&[monitor("key-a", vec![Tier::Facts]).grant_key(Tier::Facts)]);
        assert_eq!(
            monitor("key-a", vec![Tier::Facts]).granted(&saved),
            [Tier::Facts]
        );
    }

    #[test]
    fn a_package_installed_again_after_remove_asks_again() {
        let first = monitor("key-a", vec![Tier::Apps]);
        let saved = saving(&[first.grant_key(Tier::Apps)]);
        let again = Identity {
            install: "install-b".into(),
            ..first
        };
        assert!(again.granted(&saved).is_empty());
    }

    #[test]
    fn a_grant_key_names_the_app_and_the_tier() {
        let key = monitor("key-a", vec![Tier::Apps]).grant_key(Tier::Apps);
        assert!(key.starts_with("dev.example.monitor."));
        assert!(key.ends_with(".subspace.system.apps"));
    }
}
