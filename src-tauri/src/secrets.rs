use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result};
use keyring::Entry;
use tauri::{AppHandle, Manager};

/// Keychain service name for all Trawl secrets.
const SERVICE: &str = "trawl";

/// Lock for read-modify-write atomicity on the secrets index file.
static INDEX_LOCK: Mutex<()> = Mutex::new(());

fn index_path(data_dir: &Path) -> PathBuf {
    data_dir.join("secrets.json")
}

/// Names of stored secrets. The Keychain cannot enumerate entries, so an
/// index of names lives next to the other app data.
pub fn list_names(data_dir: &Path) -> Vec<String> {
    std::fs::read_to_string(index_path(data_dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_names(data_dir: &Path, names: &[String]) -> Result<()> {
    std::fs::create_dir_all(data_dir).context("create data dir")?;
    std::fs::write(index_path(data_dir), serde_json::to_string_pretty(names)?)
        .context("write secrets.json")?;
    Ok(())
}

pub fn get(name: &str) -> Result<Option<String>> {
    match Entry::new(SERVICE, name)?.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn set(data_dir: &Path, name: &str, value: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        anyhow::bail!("secret name is empty");
    }
    Entry::new(SERVICE, name)?.set_password(value)?;
    let _guard = INDEX_LOCK.lock().unwrap();
    let mut names = list_names(data_dir);
    if !names.iter().any(|n| n == name) {
        names.push(name.to_string());
        names.sort();
        save_names(data_dir, &names)?;
    }
    Ok(())
}

pub fn delete(data_dir: &Path, name: &str) -> Result<()> {
    match Entry::new(SERVICE, name)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => {}
        Err(e) => return Err(e.into()),
    }
    let _guard = INDEX_LOCK.lock().unwrap();
    let names: Vec<String> = list_names(data_dir).into_iter().filter(|n| n != name).collect();
    save_names(data_dir, &names)
}

fn data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_data_dir().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn secrets_list(app: AppHandle) -> Result<Vec<String>, String> {
    Ok(list_names(&data_dir(&app)?))
}

#[tauri::command]
pub fn secret_get(name: String) -> Result<Option<String>, String> {
    get(&name).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn secret_set(app: AppHandle, name: String, value: String) -> Result<(), String> {
    set(&data_dir(&app)?, &name, &value).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn secret_delete(app: AppHandle, name: String) -> Result<(), String> {
    delete(&data_dir(&app)?, &name).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use keyring::credential::{CredentialApi, CredentialBuilderApi};

    /// Custom in-memory credential store for testing.
    /// Persists passwords across Entry::new() calls via a shared HashMap.
    struct TestCredential {
        service: String,
        account: String,
        store: std::sync::Arc<Mutex<HashMap<(String, String), String>>>,
    }

    impl CredentialApi for TestCredential {
        fn set_password(&self, password: &str) -> keyring::error::Result<()> {
            let mut store = self.store.lock().unwrap();
            store.insert((self.service.clone(), self.account.clone()), password.to_string());
            Ok(())
        }

        fn set_secret(&self, secret: &[u8]) -> keyring::error::Result<()> {
            let mut store = self.store.lock().unwrap();
            let s = String::from_utf8(secret.to_vec())
                .map_err(|e| keyring::Error::BadEncoding(e.into_bytes()))?;
            store.insert((self.service.clone(), self.account.clone()), s);
            Ok(())
        }

        fn get_password(&self) -> keyring::error::Result<String> {
            let store = self.store.lock().unwrap();
            store
                .get(&(self.service.clone(), self.account.clone()))
                .cloned()
                .ok_or(keyring::Error::NoEntry)
        }

        fn get_secret(&self) -> keyring::error::Result<Vec<u8>> {
            let store = self.store.lock().unwrap();
            store
                .get(&(self.service.clone(), self.account.clone()))
                .map(|s| s.as_bytes().to_vec())
                .ok_or(keyring::Error::NoEntry)
        }

        fn delete_credential(&self) -> keyring::error::Result<()> {
            let mut store = self.store.lock().unwrap();
            store
                .remove(&(self.service.clone(), self.account.clone()))
                .ok_or(keyring::Error::NoEntry)?;
            Ok(())
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        fn debug_fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("TestCredential")
                .field("service", &self.service)
                .field("account", &self.account)
                .finish()
        }
    }

    struct TestCredentialBuilder {
        store: std::sync::Arc<Mutex<HashMap<(String, String), String>>>,
    }

    impl CredentialBuilderApi for TestCredentialBuilder {
        fn build(
            &self,
            _target: Option<&str>,
            service: &str,
            account: &str,
        ) -> keyring::error::Result<Box<dyn CredentialApi + Send + Sync>> {
            Ok(Box::new(TestCredential {
                service: service.to_string(),
                account: account.to_string(),
                store: self.store.clone(),
            }))
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        fn persistence(&self) -> keyring::credential::CredentialPersistence {
            keyring::credential::CredentialPersistence::EntryOnly
        }
    }

    /// Set up the test credential builder with a shared in-memory store.
    fn mock_store() {
        use std::sync::Once;
        static ONCE: Once = Once::new();
        ONCE.call_once(|| {
            let store = std::sync::Arc::new(Mutex::new(HashMap::new()));
            let builder = TestCredentialBuilder { store };
            keyring::set_default_credential_builder(Box::new(builder));
        });
    }

    fn tmp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("trawl-secrets-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn set_get_roundtrip_and_index() {
        mock_store();
        let dir = tmp_dir("roundtrip");
        set(&dir, "TG_BOT_TOKEN", "12345:abc").unwrap();
        assert_eq!(get("TG_BOT_TOKEN").unwrap().as_deref(), Some("12345:abc"));
        assert_eq!(list_names(&dir), vec!["TG_BOT_TOKEN".to_string()]);
        // Overwrite keeps a single index entry.
        set(&dir, "TG_BOT_TOKEN", "67890:def").unwrap();
        assert_eq!(get("TG_BOT_TOKEN").unwrap().as_deref(), Some("67890:def"));
        assert_eq!(list_names(&dir).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_secret_is_none() {
        mock_store();
        assert_eq!(get("TRAWL_TEST_MISSING").unwrap(), None);
    }

    #[test]
    fn delete_removes_value_and_name() {
        mock_store();
        let dir = tmp_dir("delete");
        set(&dir, "TRAWL_TEST_DEL", "1").unwrap();
        delete(&dir, "TRAWL_TEST_DEL").unwrap();
        assert_eq!(get("TRAWL_TEST_DEL").unwrap(), None);
        assert!(list_names(&dir).is_empty());
        // Deleting a missing secret is not an error.
        delete(&dir, "TRAWL_TEST_DEL").unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_name_rejected() {
        mock_store();
        let dir = tmp_dir("empty");
        assert!(set(&dir, "  ", "x").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
