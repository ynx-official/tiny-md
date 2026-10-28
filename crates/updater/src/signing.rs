//! A detached Ed25519 signature authenticates the exact manifest bytes, including
//! version, canonical URLs, lengths and hashes. The private key never ships.
use anyhow::{Result, anyhow, ensure};
use ring::signature::{ED25519, UnparsedPublicKey};

pub const PUBLIC_KEY: &str = include_str!("../update-public-key.hex");

pub fn decode_hex(text: &str) -> Result<Vec<u8>> {
    let text = text.trim();
    ensure!(
        text.len().is_multiple_of(2) && text.bytes().all(|c| c.is_ascii_hexdigit()),
        "签名编码无效"
    );
    text.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair)?, 16).map_err(Into::into))
        .collect()
}

pub fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn verify(manifest: &[u8], signature: &str, public_key: &str) -> Result<()> {
    let key = decode_hex(public_key)?;
    let signature = decode_hex(signature)?;
    ensure!(
        key.len() == 32 && signature.len() == 64,
        "发布签名或公钥长度无效"
    );
    UnparsedPublicKey::new(&ED25519, key)
        .verify(manifest, &signature)
        .map_err(|_| anyhow!("更新清单发布者签名校验失败"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::{Ed25519KeyPair, KeyPair};

    #[test]
    fn signatures_reject_changed_bytes_wrong_key_and_invalid_encodings() {
        let key = Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
        let public = encode_hex(key.public_key().as_ref());
        let signature = encode_hex(key.sign(b"manifest\n").as_ref());
        verify(b"manifest\n", &signature, &public).unwrap();
        assert!(verify(b"manifest", &signature, &public).is_err());
        assert!(verify(b"manifest\n", &signature, &"00".repeat(32)).is_err());
        for bad in ["", "xyz", "00", "中文"] {
            assert!(verify(b"manifest\n", bad, &public).is_err());
        }
    }
}
