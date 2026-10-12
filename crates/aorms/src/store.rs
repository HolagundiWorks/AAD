//! Where the sign-in lives between runs. The refresh token is shared with the other AORMS desktop apps on the machine
//! (one OS credential, target `AORMS/Connect`); each app keeps only its own session id separately. See AORMS-SUITE-SIGNIN.md.
use std::sync::{Arc, Mutex};

/// The shared sign-in: blob `{ "v":1, "refreshToken", "email", "updatedAt" }` (`updatedAt` = Unix seconds, an integer).
#[derive(Debug, Clone, PartialEq)]
pub struct SharedSignIn {
    pub refresh_token: String,
    pub email: Option<String>,
    /// Unix seconds.
    pub updated_at: i64,
}

pub trait SharedSignInStore: Send {
    fn load(&self) -> Option<SharedSignIn>;
    fn save(&self, s: &SharedSignIn);
    fn clear(&self);
}

/// This app's session id (never shared: each app has its own session).
pub trait SessionStore: Send {
    fn load(&self) -> Option<String>;
    fn save(&self, session_id: &str);
    fn clear(&self);
}

/// In-memory stores: for tests, and for runs that must not persist. Clones share the same state, so two clients in one process can
/// stand in for two apps on one machine.
#[derive(Debug, Clone, Default)]
pub struct MemorySharedStore(Arc<Mutex<Option<SharedSignIn>>>);

impl SharedSignInStore for MemorySharedStore {
    fn load(&self) -> Option<SharedSignIn> {
        self.0.lock().ok().and_then(|g| g.clone())
    }
    fn save(&self, s: &SharedSignIn) {
        if let Ok(mut g) = self.0.lock() {
            *g = Some(s.clone());
        }
    }
    fn clear(&self) {
        if let Ok(mut g) = self.0.lock() {
            *g = None;
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct MemorySessionStore(Arc<Mutex<Option<String>>>);

impl SessionStore for MemorySessionStore {
    fn load(&self) -> Option<String> {
        self.0.lock().ok().and_then(|g| g.clone())
    }
    fn save(&self, id: &str) {
        if let Ok(mut g) = self.0.lock() {
            *g = Some(id.to_string());
        }
    }
    fn clear(&self) {
        if let Ok(mut g) = self.0.lock() {
            *g = None;
        }
    }
}

#[cfg(any(test, all(feature = "keyring-store", not(target_arch = "wasm32"))))]
pub(crate) fn encode(s: &SharedSignIn) -> String {
    serde_json::json!({ "v": 1, "refreshToken": s.refresh_token, "email": s.email, "updatedAt": s.updated_at }).to_string()
}

#[cfg(any(test, all(feature = "keyring-store", not(target_arch = "wasm32"))))]
pub(crate) fn decode(text: &str) -> Option<SharedSignIn> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    if v.get("v").and_then(serde_json::Value::as_i64) != Some(1) {
        return None; // a newer layout: leave it alone rather than guess
    }
    let token = v.get("refreshToken")?.as_str().filter(|t| !t.is_empty())?;
    Some(SharedSignIn {
        refresh_token: token.to_string(),
        email: v.get("email").and_then(serde_json::Value::as_str).map(str::to_string),
        updated_at: v.get("updatedAt").and_then(serde_json::Value::as_i64).unwrap_or(0),
    })
}

/// The OS credential store: Windows Credential Manager (target `AORMS/Connect`, shared with AQC), macOS Keychain, Linux keyutils.
/// Not exercised by the automated tests (no credential store in CI); verify by hand on Windows with AQC installed.
#[cfg(all(feature = "keyring-store", not(target_arch = "wasm32")))]
pub struct KeyringStore {
    service: String,
    user: String,
}

#[cfg(all(feature = "keyring-store", not(target_arch = "wasm32")))]
impl KeyringStore {
    /// The sign-in shared by every suite app.
    pub fn shared() -> Self {
        KeyringStore { service: "AORMS/Connect".into(), user: "sign-in".into() }
    }
    /// AAD's own session id.
    pub fn session() -> Self {
        KeyringStore { service: "AORMS/Connect/aad".into(), user: "session".into() }
    }
    fn entry(&self) -> Option<keyring::Entry> {
        keyring::Entry::new(&self.service, &self.user).ok()
    }
    fn read(&self) -> Option<String> {
        self.entry()?.get_password().ok()
    }
    fn write(&self, text: &str) {
        if let Some(e) = self.entry() {
            let _ = e.set_password(text);
        }
    }
    fn delete(&self) {
        if let Some(e) = self.entry() {
            let _ = e.delete_credential();
        }
    }
}

#[cfg(all(feature = "keyring-store", not(target_arch = "wasm32")))]
impl SharedSignInStore for KeyringStore {
    fn load(&self) -> Option<SharedSignIn> {
        decode(&self.read()?)
    }
    fn save(&self, s: &SharedSignIn) {
        self.write(&encode(s));
    }
    fn clear(&self) {
        self.delete();
    }
}

#[cfg(all(feature = "keyring-store", not(target_arch = "wasm32")))]
impl SessionStore for KeyringStore {
    fn load(&self) -> Option<String> {
        self.read().filter(|t| !t.is_empty())
    }
    fn save(&self, id: &str) {
        self.write(id);
    }
    fn clear(&self) {
        self.delete();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_blob_round_trips_and_has_the_documented_shape() {
        let s = SharedSignIn { refresh_token: "r1".into(), email: Some("a@x.in".into()), updated_at: 1_700_000_000 };
        let text = encode(&s);
        let v: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
        assert_eq!(
            (v["v"].as_i64(), v["refreshToken"].as_str(), v["email"].as_str(), v["updatedAt"].as_i64()),
            (Some(1), Some("r1"), Some("a@x.in"), Some(1_700_000_000))
        );
        assert_eq!(decode(&text), Some(s));
    }

    #[test]
    fn what_another_app_wrote_is_read_defensively() {
        assert_eq!(decode(r#"{"v":1,"refreshToken":"r","updatedAt":"later"}"#).map(|s| (s.updated_at, s.email)), Some((0, None)));
        assert!(decode(r#"{"v":2,"refreshToken":"r"}"#).is_none()); // a newer layout is left alone, not guessed
        assert!(decode(r#"{"v":1,"refreshToken":""}"#).is_none());
        assert!(decode("not json").is_none());
        assert!(decode("").is_none());
    }

    #[test]
    fn clones_of_a_memory_store_share_state_like_two_apps_on_one_machine() {
        let a = MemorySharedStore::default();
        let b = a.clone();
        a.save(&SharedSignIn { refresh_token: "r".into(), email: None, updated_at: 1 });
        assert_eq!(b.load().map(|s| s.refresh_token), Some("r".into()));
        b.clear();
        assert!(a.load().is_none());
    }
}
