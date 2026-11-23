use std::{
    fmt, fs,
    io::Write,
    path::{Path, PathBuf},
};

use chacha20poly1305::{
    Key, KeyInit, XChaCha20Poly1305, XNonce,
    aead::{Aead, Payload},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

const CREDENTIAL_MAGIC: &[u8; 4] = b"SHIR";
const CREDENTIAL_FORMAT_VERSION: u8 = 1;
const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 24;
const KEY_FILE_NAME: &str = "credentials.key";

/// Cloud authentication credentials held exclusively inside Rust compatibility layer.
///
/// Secrets are redacted from `Debug` output and zeroized in memory on drop.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct CloudCredentials {
    pub origin: String,
    pub user_id: String,
    pub username: String,
    pub access_token: String,
    pub refresh_token: String,
}

impl fmt::Debug for CloudCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CloudCredentials")
            .field("origin", &self.origin)
            .field("user_id", &self.user_id)
            .field("username", &self.username)
            .field("access_token", &"[REDACTED]")
            .field("refresh_token", &"[REDACTED]")
            .finish()
    }
}

/// Public, non-secret cloud account identity safe to expose across IPC / UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudUserInfo {
    pub user_id: String,
    pub username: String,
}

impl From<&CloudCredentials> for CloudUserInfo {
    fn from(value: &CloudCredentials) -> Self {
        Self {
            user_id: value.user_id.clone(),
            username: value.username.clone(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CredentialStoreError {
    #[error("credential encryption key missing at {}", .0.display())]
    KeyMissing(PathBuf),

    #[error("encrypted credential store corrupted: {0}")]
    Corrupted(String),

    #[error("credential store I/O failure: {0}")]
    Io(#[from] std::io::Error),
}

/// Abstract credential storage port allowing future migration to Windows Credential Manager
/// and Android Keystore without changing callers.
pub trait CredentialStore: Send + Sync {
    fn load(&self, origin: &str) -> Result<Option<CloudCredentials>, CredentialStoreError>;
    fn save(
        &self,
        origin: &str,
        credentials: &CloudCredentials,
    ) -> Result<(), CredentialStoreError>;
    fn clear(&self, origin: &str) -> Result<(), CredentialStoreError>;
}

/// First-edition credential store protecting credentials with `XChaCha20Poly1305` (24-byte random
/// nonce + origin-bound AAD), storing the random key file separately from per-origin ciphertext
/// files with restrictive owner-only filesystem permissions on Unix.
#[derive(Debug, Clone)]
pub struct EncryptedFileCredentialStore {
    key_dir: PathBuf,
    key_path: PathBuf,
    ciphertext_dir: PathBuf,
}

impl EncryptedFileCredentialStore {
    pub fn new(key_dir: impl Into<PathBuf>, ciphertext_dir: impl Into<PathBuf>) -> Self {
        let key_dir = key_dir.into();
        let key_path = key_dir.join(KEY_FILE_NAME);
        let ciphertext_dir = ciphertext_dir.into();
        Self {
            key_dir,
            key_path,
            ciphertext_dir,
        }
    }

    pub fn key_path(&self) -> &Path {
        &self.key_path
    }

    pub fn ciphertext_path_for_origin(&self, origin: &str) -> PathBuf {
        let digest = Sha256::digest(origin.as_bytes());
        let hex = encode_hex(&digest);
        self.ciphertext_dir.join(format!("{hex}.enc"))
    }

    fn load_key(&self) -> Result<Zeroizing<[u8; KEY_LEN]>, CredentialStoreError> {
        if !self.key_path.exists() {
            return Err(CredentialStoreError::KeyMissing(self.key_path.clone()));
        }
        let bytes = Zeroizing::new(fs::read(&self.key_path)?);
        if bytes.len() != KEY_LEN {
            return Err(CredentialStoreError::Corrupted(format!(
                "invalid key length {} (expected {KEY_LEN})",
                bytes.len()
            )));
        }
        let mut key = Zeroizing::new([0u8; KEY_LEN]);
        key.copy_from_slice(&bytes);
        Ok(key)
    }

    fn load_or_create_key(&self) -> Result<Zeroizing<[u8; KEY_LEN]>, CredentialStoreError> {
        if self.key_path.exists() {
            match self.load_key() {
                Ok(key) => return Ok(key),
                Err(CredentialStoreError::Corrupted(_)) => {
                    // Explicit overwrite on fresh save after corruption so re-login always recovers.
                }
                Err(err) => return Err(err),
            }
        }

        ensure_private_dir(&self.key_dir)?;
        let mut raw_key = Zeroizing::new([0u8; KEY_LEN]);
        getrandom::fill(raw_key.as_mut())
            .map_err(|err| std::io::Error::other(format!("OS CSPRNG failed: {err}")))?;
        write_private_file_atomic(&self.key_dir, &self.key_path, raw_key.as_slice())?;
        Ok(raw_key)
    }
}

impl CredentialStore for EncryptedFileCredentialStore {
    fn load(&self, origin: &str) -> Result<Option<CloudCredentials>, CredentialStoreError> {
        let ciphertext_path = self.ciphertext_path_for_origin(origin);
        if !ciphertext_path.exists() {
            return Ok(None);
        }

        let key_bytes = self.load_key()?;
        let envelope = fs::read(&ciphertext_path)?;

        let min_len = CREDENTIAL_MAGIC.len() + 1 + NONCE_LEN + 16;
        if envelope.len() < min_len {
            return Err(CredentialStoreError::Corrupted(
                "credential ciphertext truncated".to_owned(),
            ));
        }
        if &envelope[..CREDENTIAL_MAGIC.len()] != CREDENTIAL_MAGIC {
            return Err(CredentialStoreError::Corrupted(
                "invalid credential envelope magic".to_owned(),
            ));
        }
        let version = envelope[CREDENTIAL_MAGIC.len()];
        if version != CREDENTIAL_FORMAT_VERSION {
            return Err(CredentialStoreError::Corrupted(format!(
                "unsupported credential envelope version {version}"
            )));
        }

        let nonce_start = CREDENTIAL_MAGIC.len() + 1;
        let nonce_end = nonce_start + NONCE_LEN;
        let mut nonce_arr = [0u8; NONCE_LEN];
        nonce_arr.copy_from_slice(&envelope[nonce_start..nonce_end]);
        let ciphertext = &envelope[nonce_end..];

        let key = Key::from(*key_bytes);
        let cipher = XChaCha20Poly1305::new(&key);
        let nonce = XNonce::from(nonce_arr);
        let aad = credential_aad(origin);

        let plaintext = Zeroizing::new(
            cipher
                .decrypt(
                    &nonce,
                    Payload {
                        msg: ciphertext,
                        aad: aad.as_bytes(),
                    },
                )
                .map_err(|_| {
                    CredentialStoreError::Corrupted(
                        "AEAD authentication tag verification failed".to_owned(),
                    )
                })?,
        );

        let creds: CloudCredentials = serde_json::from_slice(&plaintext).map_err(|err| {
            CredentialStoreError::Corrupted(format!("invalid credential payload JSON: {err}"))
        })?;
        if creds.origin != origin {
            return Err(CredentialStoreError::Corrupted(
                "credential payload origin mismatch".to_owned(),
            ));
        }

        Ok(Some(creds))
    }

    fn save(
        &self,
        origin: &str,
        credentials: &CloudCredentials,
    ) -> Result<(), CredentialStoreError> {
        ensure_private_dir(&self.ciphertext_dir)?;
        let key_bytes = self.load_or_create_key()?;

        let mut nonce_arr = [0u8; NONCE_LEN];
        getrandom::fill(&mut nonce_arr)
            .map_err(|err| std::io::Error::other(format!("OS CSPRNG failed: {err}")))?;

        let plaintext = Zeroizing::new(serde_json::to_vec(credentials).map_err(|err| {
            std::io::Error::other(format!("failed to serialize credentials: {err}"))
        })?);

        let key = Key::from(*key_bytes);
        let cipher = XChaCha20Poly1305::new(&key);
        let nonce = XNonce::from(nonce_arr);
        let aad = credential_aad(origin);

        let ciphertext = cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: plaintext.as_slice(),
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| std::io::Error::other("AEAD encryption failed"))?;

        let mut envelope =
            Vec::with_capacity(CREDENTIAL_MAGIC.len() + 1 + NONCE_LEN + ciphertext.len());
        envelope.extend_from_slice(CREDENTIAL_MAGIC);
        envelope.push(CREDENTIAL_FORMAT_VERSION);
        envelope.extend_from_slice(&nonce_arr);
        envelope.extend_from_slice(&ciphertext);

        let ciphertext_path = self.ciphertext_path_for_origin(origin);
        write_private_file_atomic(&self.ciphertext_dir, &ciphertext_path, &envelope)?;
        Ok(())
    }

    fn clear(&self, origin: &str) -> Result<(), CredentialStoreError> {
        let ciphertext_path = self.ciphertext_path_for_origin(origin);
        match fs::remove_file(&ciphertext_path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(CredentialStoreError::Io(err)),
        }
    }
}

fn credential_aad(origin: &str) -> String {
    format!("shiro-reader:cloud-credentials:v{CREDENTIAL_FORMAT_VERSION}:{origin}")
}

fn ensure_private_dir(dir: &Path) -> Result<(), std::io::Error> {
    fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn write_private_file_atomic(
    dir: &Path,
    target_path: &Path,
    bytes: &[u8],
) -> Result<(), std::io::Error> {
    let mut temp = NamedTempFile::new_in(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temp.as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(target_path).map_err(|err| err.error)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(target_path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_creds(origin: &str) -> CloudCredentials {
        CloudCredentials {
            origin: origin.to_owned(),
            user_id: "u-1".to_owned(),
            username: "Alice".to_owned(),
            access_token: "secret-access-token".to_owned(),
            refresh_token: "secret-refresh-token".to_owned(),
        }
    }

    #[test]
    fn debug_redacts_tokens() {
        let creds = sample_creds("https://books.example.com");
        let rendered = format!("{creds:?}");
        assert!(rendered.contains("Alice"));
        assert!(!rendered.contains("secret-access-token"));
        assert!(!rendered.contains("secret-refresh-token"));
    }

    #[test]
    fn encrypts_separates_key_and_detects_corruption_and_missing_key() {
        let tmp = tempfile::tempdir().unwrap();
        let store =
            EncryptedFileCredentialStore::new(tmp.path().join("keys"), tmp.path().join("creds"));
        let origin = "https://books.example.com";

        assert!(store.load(origin).unwrap().is_none());

        let creds = sample_creds(origin);
        store.save(origin, &creds).unwrap();

        assert!(store.key_path().exists());
        let enc_path = store.ciphertext_path_for_origin(origin);
        assert!(enc_path.exists());
        assert_ne!(store.key_path().parent(), enc_path.parent());

        // Ciphertext on disk must never contain plaintext tokens
        let raw_enc = fs::read(&enc_path).unwrap();
        assert!(
            !raw_enc
                .windows(b"secret-access-token".len())
                .any(|w| w == b"secret-access-token")
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let key_mode = fs::metadata(store.key_path()).unwrap().permissions().mode() & 0o777;
            let enc_mode = fs::metadata(&enc_path).unwrap().permissions().mode() & 0o777;
            assert_eq!(key_mode, 0o600);
            assert_eq!(enc_mode, 0o600);
        }

        // Roundtrip succeeds
        let loaded = store.load(origin).unwrap().unwrap();
        assert_eq!(loaded, creds);

        // Moving ciphertext to another origin path fails AAD verification
        let other_origin = "https://other.example.com";
        let other_path = store.ciphertext_path_for_origin(other_origin);
        fs::copy(&enc_path, &other_path).unwrap();
        assert!(matches!(
            store.load(other_origin),
            Err(CredentialStoreError::Corrupted(_))
        ));
        store.clear(other_origin).unwrap();

        // Tampering a ciphertext byte fails AEAD verification
        let mut tampered = raw_enc.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 0x55;
        fs::write(&enc_path, tampered).unwrap();
        assert!(matches!(
            store.load(origin),
            Err(CredentialStoreError::Corrupted(_))
        ));

        // Restore valid ciphertext and delete key -> explicit KeyMissing error
        fs::write(&enc_path, raw_enc).unwrap();
        fs::remove_file(store.key_path()).unwrap();
        assert!(matches!(
            store.load(origin),
            Err(CredentialStoreError::KeyMissing(_))
        ));

        // Re-saving generates a new key and recovers cleanly
        store.save(origin, &creds).unwrap();
        assert_eq!(store.load(origin).unwrap().unwrap(), creds);

        // Clear removes ciphertext idempotently
        store.clear(origin).unwrap();
        store.clear(origin).unwrap();
        assert!(store.load(origin).unwrap().is_none());
    }
}
