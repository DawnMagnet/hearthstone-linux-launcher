use crate::{paths::AppPaths, AppConfig};
use aes::Aes128;
use anyhow::{Context, Result};
use cbc::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
use pbkdf2::pbkdf2_hmac;
use sha1::Sha1;
use std::path::Path;
use url::Url;

type Aes128CbcEnc = cbc::Encryptor<Aes128>;

const ENTROPY: [u8; 16] = [
    200, 118, 244, 174, 76, 149, 46, 254, 242, 250, 15, 84, 25, 192, 156, 67,
];
const SALT: &[u8] = b"someSalt";
const ITERATIONS: u32 = 1000;
const TOKEN_CIPHERTEXT_LEN: usize = 0x30;

pub fn extract_token_from_uri(uri: &str) -> Result<String> {
    if let Ok(url) = Url::parse(uri) {
        for (_, value) in url.query_pairs() {
            if looks_like_token(&value) {
                return Ok(value.into_owned());
            }
        }
    }

    find_token_candidate(uri).context("no Hearthstone login token found in callback URI")
}

pub fn looks_like_token(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 45
        && bytes[2] == b'-'
        && bytes[35] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(idx, byte)| idx == 2 || idx == 35 || byte.is_ascii_alphanumeric())
}

fn find_token_candidate(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    if bytes.len() < 45 {
        return None;
    }

    for start in 0..=(bytes.len() - 45) {
        let candidate = &input[start..start + 45];
        if looks_like_token(candidate) {
            return Some(candidate.to_string());
        }
    }
    None
}

pub fn write_encrypted_token_for_current_user(path: &Path, token: &str) -> Result<()> {
    let username = current_username();
    let encrypted = encrypt_token_for_user(token, &username)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, encrypted).with_context(|| format!("failed to write {}", path.display()))
}

pub fn handle_callback_uri(paths: &AppPaths, uri: &str) -> Result<()> {
    let mut config = AppConfig::load_or_default(&paths.config_file)?;
    let game_dir = config.game_dir.clone().unwrap_or(paths.game_dir.clone());
    let token = extract_token_from_uri(uri)?;
    write_encrypted_token_for_current_user(&game_dir.join("token"), &token)?;
    config.game_dir = Some(game_dir);
    config.logged_in = true;
    config.last_login_at = Some(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs()
            .to_string(),
    );
    config.save(&paths.config_file)
}

pub fn encrypt_token_for_user(token: &str, username: &str) -> Result<Vec<u8>> {
    anyhow::ensure!(looks_like_token(token), "token format is invalid");

    let key = encryption_key_for_user(username);
    let iv = [0u8; 16];
    let ciphertext = Aes128CbcEnc::new(&key.into(), &iv.into())
        .encrypt_padded_vec_mut::<Pkcs7>(token.as_bytes());

    anyhow::ensure!(
        ciphertext.len() == TOKEN_CIPHERTEXT_LEN,
        "unexpected encrypted token length {}",
        ciphertext.len()
    );
    Ok(ciphertext)
}

pub fn encryption_key_for_user(username: &str) -> [u8; 16] {
    let mut entropy = ENTROPY;
    for (idx, byte) in username.as_bytes().iter().take(entropy.len()).enumerate() {
        entropy[idx] ^= *byte;
    }

    let mut key = [0u8; 16];
    pbkdf2_hmac::<Sha1>(&entropy, SALT, ITERATIONS, &mut key);
    key
}

fn current_username() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "user".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_token_from_query_or_text() {
        let token = "AB-0123456789ABCDEFGHIJKLMNOPQRSTUV-123456789";
        assert_eq!(
            extract_token_from_uri(&format!("wtcg://login?ST={token}&foo=bar")).unwrap(),
            token
        );
        assert_eq!(
            extract_token_from_uri(&format!("copy this {token} please")).unwrap(),
            token
        );
    }

    #[test]
    fn encrypts_to_game_expected_length() {
        let token = "AB-0123456789ABCDEFGHIJKLMNOPQRSTUV-123456789";
        let encrypted = encrypt_token_for_user(token, "sgct").unwrap();
        assert_eq!(encrypted.len(), TOKEN_CIPHERTEXT_LEN);
    }
}
