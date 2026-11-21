use super::{Session, decode};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use std::time::{SystemTime, UNIX_EPOCH};
use subspace_account::{CredentialStore as _, MemoryStore};

mod browser;
mod device;

fn jwt(lifetime: u64) -> String {
    let exp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + lifetime;
    let payload = URL_SAFE_NO_PAD.encode(format!("{{\"exp\":{exp}}}"));
    format!("e30.{payload}.signature")
}

fn stored(store: &MemoryStore) -> Session {
    decode(&store.load().unwrap().unwrap()).unwrap()
}
