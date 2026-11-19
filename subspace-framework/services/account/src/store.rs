use crate::Error;
use std::sync::Mutex;
use subspace_core::{Secret, keychain::Keychain};

/// The only persistence boundary. Implementations must protect credential values.
pub trait CredentialStore: Send + Sync {
    fn load(&self) -> Result<Option<Secret>, Error>;
    fn save(&self, value: &Secret) -> Result<(), Error>;
    fn delete(&self) -> Result<(), Error>;
}

/// Always uses the native OS store, including development builds.
pub struct SystemStore(Keychain);

impl SystemStore {
    pub fn new(namespace: &str) -> Self {
        Self(Keychain::new(namespace))
    }
}

impl CredentialStore for SystemStore {
    fn load(&self) -> Result<Option<Secret>, Error> {
        self.0.load("session").map_err(|_| Error::Keychain)
    }
    fn save(&self, value: &Secret) -> Result<(), Error> {
        self.0.save("session", value).map_err(|_| Error::Keychain)
    }
    fn delete(&self) -> Result<(), Error> {
        self.0.delete("session").map_err(|_| Error::Keychain)
    }
}

/// Process memory only: for tests and local development issuers whose
/// sessions must not outlive the app.
// qual:api — for app providers and tests that must not persist.
#[derive(Default)]
pub struct MemoryStore(Mutex<Option<Secret>>);

impl CredentialStore for MemoryStore {
    fn load(&self) -> Result<Option<Secret>, Error> {
        Ok(self.0.lock().map_err(|_| Error::Service)?.clone())
    }
    fn save(&self, value: &Secret) -> Result<(), Error> {
        *self.0.lock().map_err(|_| Error::Service)? = Some(value.clone());
        Ok(())
    }
    fn delete(&self) -> Result<(), Error> {
        *self.0.lock().map_err(|_| Error::Service)? = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{CredentialStore as _, MemoryStore};
    use subspace_core::Secret;

    #[test]
    fn the_memory_store_keeps_one_value_until_it_is_deleted() {
        let store = MemoryStore::default();
        assert!(store.load().unwrap().is_none());
        store.save(&Secret::new("fixture")).unwrap();
        assert_eq!(store.load().unwrap().unwrap().expose(), "fixture");
        store.delete().unwrap();
        assert!(store.load().unwrap().is_none());
    }
}
