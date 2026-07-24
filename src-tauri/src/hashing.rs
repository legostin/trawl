//! Hashing and base64 primitives backing the script stdlib
//! (`__native_digest` / `__native_base64` in scripting.rs).

use base64::Engine;
use hmac::Mac;
use sha2::Digest;

/// kind: "sha256" | "md5" | "hmac-sha256" (key used only for hmac).
/// Returns lowercase hex, or None for an unknown kind.
pub fn digest(kind: &str, key: &str, data: &str) -> Option<String> {
    match kind {
        "sha256" => Some(hex::encode(sha2::Sha256::digest(data.as_bytes()))),
        "md5" => Some(hex::encode(md5::Md5::digest(data.as_bytes()))),
        "hmac-sha256" => {
            let mut mac = hmac::Hmac::<sha2::Sha256>::new_from_slice(key.as_bytes()).ok()?;
            mac.update(data.as_bytes());
            Some(hex::encode(mac.finalize().into_bytes()))
        }
        _ => None,
    }
}

/// op: "encode" | "decode". decode accepts standard and base64url alphabets,
/// with or without padding; None on invalid input, non-UTF-8 result, or unknown op.
pub fn base64(op: &str, data: &str) -> Option<String> {
    match op {
        "encode" => Some(base64::engine::general_purpose::STANDARD.encode(data.as_bytes())),
        "decode" => {
            let mut normalized: String = data
                .chars()
                .filter(|c| !c.is_whitespace())
                .map(|c| match c {
                    '-' => '+',
                    '_' => '/',
                    c => c,
                })
                .collect();
            normalized = normalized.trim_end_matches('=').to_string();
            while normalized.len() % 4 != 0 {
                normalized.push('=');
            }
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(normalized.as_bytes())
                .ok()?;
            String::from_utf8(bytes).ok()
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_known_vector() {
        assert_eq!(
            digest("sha256", "", "abc").as_deref(),
            Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        );
    }

    #[test]
    fn md5_known_vector() {
        assert_eq!(
            digest("md5", "", "abc").as_deref(),
            Some("900150983cd24fb0d6963f7d28e17f72")
        );
    }

    #[test]
    fn hmac_sha256_rfc4231_vector() {
        // RFC 4231 test case 2: key "Jefe", data "what do ya want for nothing?"
        assert_eq!(
            digest("hmac-sha256", "Jefe", "what do ya want for nothing?").as_deref(),
            Some("5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843")
        );
    }

    #[test]
    fn digest_unknown_kind_is_none() {
        assert_eq!(digest("sha1", "", "abc"), None);
    }

    #[test]
    fn base64_encode_basic() {
        assert_eq!(base64("encode", "hello").as_deref(), Some("aGVsbG8="));
    }

    #[test]
    fn base64_decode_standard() {
        assert_eq!(base64("decode", "aGVsbG8=").as_deref(), Some("hello"));
    }

    #[test]
    fn base64_decode_unpadded_and_url_alphabet() {
        // JWT header segment: {"alg":"HS256","typ":"JWT"} — base64url without padding.
        assert_eq!(
            base64("decode", "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9").as_deref(),
            Some(r#"{"alg":"HS256","typ":"JWT"}"#)
        );
        // base64url characters - and _ (bytes 0xfb 0xef 0xff are not UTF-8, so use a
        // URL-safe payload that decodes to text: ">>>???" is Pj4+Pz8/ std, Pj4-Pz8_ url)
        assert_eq!(base64("decode", "Pj4-Pz8_").as_deref(), Some(">>>???"));
    }

    #[test]
    fn base64_decode_ignores_whitespace() {
        assert_eq!(base64("decode", "aGVs\nbG8=").as_deref(), Some("hello"));
    }

    #[test]
    fn base64_roundtrip() {
        let original = "user:p@ss//w+rd";
        let encoded = base64("encode", original).unwrap();
        assert_eq!(base64("decode", &encoded).as_deref(), Some(original));
    }

    #[test]
    fn base64_decode_invalid_is_none() {
        assert_eq!(base64("decode", "!!!not base64!!!"), None);
    }

    #[test]
    fn base64_unknown_op_is_none() {
        assert_eq!(base64("frobnicate", "aGVsbG8="), None);
    }
}
