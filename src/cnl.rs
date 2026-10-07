//! Click'n'Load v2 (CNL2) decoding.
//!
//! Spec: https://jdownloader.org/knowledge/wiki/glossary/cnl2
//! The links are AES-128-CBC encrypted without padding, key == IV, and the key is
//! hex encoded inside a small JavaScript function passed as `jk`.

use aes::Aes128;
use aes::cipher::{Block, BlockCipherDecrypt, KeyInit};
use anyhow::{Context, Result, bail};
use base64::Engine;
use base64::engine::general_purpose::{GeneralPurpose, PAD_INDIFFERENT};
use regex::Regex;
use std::sync::LazyLock;

const B64: GeneralPurpose = GeneralPurpose::new(&base64::alphabet::STANDARD, PAD_INDIFFERENT);

static QUOTED_KEY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"['"]([0-9a-fA-F]{32})['"]"#).unwrap());
static BARE_KEY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b([0-9a-fA-F]{32})\b").unwrap());

/// Extracts the 16 byte key from the `jk` JavaScript snippet,
/// e.g. `function f(){ return '31323334353637383930393837363534';}`.
pub fn extract_key(jk: &str) -> Result<[u8; 16]> {
    let hex_key = QUOTED_KEY
        .captures(jk)
        .or_else(|| BARE_KEY.captures(jk))
        .map(|c| c[1].to_string())
        .with_context(|| format!("no 128 bit hex key found in jk: {jk:.200}"))?;
    let mut key = [0u8; 16];
    hex::decode_to_slice(&hex_key, &mut key).context("invalid hex key")?;
    Ok(key)
}

/// Decrypts `crypted` (base64) with the given key and returns the plain text.
pub fn decrypt(crypted: &str, key: &[u8; 16]) -> Result<String> {
    // Form encoding sometimes turns '+' into ' ' when the sender did not escape it.
    let cleaned: String = crypted
        .trim()
        .chars()
        .filter(|c| !matches!(c, '\r' | '\n' | '\t'))
        .map(|c| if c == ' ' { '+' } else { c })
        .collect();
    let mut data = B64
        .decode(cleaned.as_bytes())
        .context("crypted is not valid base64")?;
    if data.is_empty() || data.len() % 16 != 0 {
        bail!(
            "crypted length {} is not a multiple of the AES block size",
            data.len()
        );
    }

    let cipher = Aes128::new_from_slice(key).expect("key has 16 bytes");
    let mut prev = *key; // IV == key
    for block in data.as_chunks_mut::<16>().0 {
        let mut current = [0u8; 16];
        current.copy_from_slice(block);
        let mut plain = Block::<Aes128>::default();
        plain.copy_from_slice(block);
        cipher.decrypt_block(&mut plain);
        for ((out, p), iv) in block.iter_mut().zip(plain.iter()).zip(prev.iter()) {
            *out = p ^ iv;
        }
        prev = current;
    }

    Ok(String::from_utf8_lossy(&data)
        .trim_end_matches('\0')
        .to_string())
}

/// Splits a newline separated link list into clean links.
pub fn split_links(text: &str) -> Vec<String> {
    text.split(['\r', '\n'])
        .map(|l| l.trim_matches(|c: char| c.is_whitespace() || c == '\0'))
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

/// Decrypts a CNL2 payload into a list of links.
pub fn decrypt_links(crypted: &str, jk: &str) -> Result<Vec<String>> {
    let key = extract_key(jk)?;
    let links = split_links(&decrypt(crypted, &key)?);
    if links.is_empty() {
        bail!("decrypted payload contains no links");
    }
    Ok(links)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test vector from the official CNL2 documentation.
    const JK: &str = "function f(){ return '31323334353637383930393837363534';}";
    const CRYPTED: &str =
        "DRurBGEf2ntP7Z0WDkMP8e1ZeK7PswJGeBHCg4zEYXZSE3Qqxsbi5EF1KosgkKQ9SL8qOOUAI+eDPFypAtQS9A==";

    #[test]
    fn extracts_key() {
        assert_eq!(&extract_key(JK).unwrap(), b"1234567890987654");
        assert_eq!(
            &extract_key(r#"function f(){return "31323334353637383930393837363534"}"#).unwrap(),
            b"1234567890987654"
        );
        assert!(extract_key("function f(){ return 'abc';}").is_err());
    }

    #[test]
    fn decrypts_doc_example() {
        let links = decrypt_links(CRYPTED, JK).unwrap();
        assert_eq!(
            links,
            vec!["http://rapidshare.com/files/285626259/jDownloader.dmg"]
        );
    }

    #[test]
    fn tolerates_unescaped_plus() {
        let mangled = CRYPTED.replace('+', " ");
        assert_eq!(
            decrypt_links(&mangled, JK).unwrap(),
            decrypt_links(CRYPTED, JK).unwrap()
        );
    }

    #[test]
    fn rejects_bad_length() {
        assert!(decrypt_links("AAAA", JK).is_err());
    }
}
