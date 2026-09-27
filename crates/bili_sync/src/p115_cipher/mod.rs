//! 115 wire protocol. Adapted from rss2pan (MIT); see THIRD_PARTY_NOTICES.md.
mod xor;
use std::sync::LazyLock;

use anyhow::{Context, Result, ensure};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use num_bigint::BigUint;
use xor::{XOR_CLIENT_KEY, xor_derive_key, xor_transform};
static MODULUS: LazyLock<BigUint> = LazyLock::new(|| {
    BigUint::parse_bytes(b"8686980c0f5a24c4b9d43020cd2c22703ff3f450756529058b1cf88f09b8602136477198a6e2683149659bd122c33592fdb5ad47944ad1ea4d36c6b172aad6338c3bb6ac6227502d010993ac967d1aef00f0c8e038de2e4d3bc2ec368af2e9f10a6f1eda4f7262f136420c07c331b871bf139f74f3010e3c4fe57df3afb71683",16).expect("115 public modulus")
});
fn transform(input: &[u8]) -> Vec<u8> {
    let out = BigUint::from_bytes_be(input)
        .modpow(&BigUint::from(65537u32), &MODULUS)
        .to_bytes_be();
    let mut padded = vec![0; 128 - out.len()];
    padded.extend(out);
    padded
}
pub fn encode(input: &[u8], key: &[u8; 16]) -> String {
    let mut payload = input.to_vec();
    xor_transform(&mut payload, &xor_derive_key(key, 4));
    payload.reverse();
    xor_transform(&mut payload, &XOR_CLIENT_KEY);
    let message = [key.as_slice(), payload.as_slice()].concat();
    let mut output = Vec::new();
    for chunk in message.chunks(117) {
        let count = 125 - chunk.len();
        let mut block = vec![0, 2];
        for _ in 0..count {
            let mut b = rand::random::<u8>();
            while b == 0 {
                b = rand::random();
            }
            block.push(b);
        }
        block.push(0);
        block.extend(chunk);
        output.extend(transform(&block));
    }
    STANDARD.encode(output)
}
pub fn decode(input: &str, key: &[u8; 16]) -> Result<Vec<u8>> {
    let encoded = STANDARD.decode(input)?;
    ensure!(
        !encoded.is_empty() && encoded.len().is_multiple_of(128) && encoded.len() <= 1024 * 1024,
        "115 播放响应长度无效"
    );
    let mut plain = Vec::new();
    for chunk in encoded.chunks(128) {
        let block = transform(chunk);
        ensure!(
            block[0] == 0 && (block[1] == 1 || block[1] == 2),
            "115 播放响应填充无效"
        );
        let zero = block[2..]
            .iter()
            .position(|b| *b == 0)
            .context("115 播放响应填充缺失")?
            + 2;
        ensure!(zero >= 10, "115 播放响应填充过短");
        plain.extend_from_slice(&block[zero + 1..]);
    }
    ensure!(plain.len() >= 16, "115 播放响应内容过短");
    let server_key = xor_derive_key(&plain[..16], 12);
    let mut payload = plain[16..].to_vec();
    xor_transform(&mut payload, &server_key);
    payload.reverse();
    xor_transform(&mut payload, &xor_derive_key(key, 4));
    Ok(payload)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_truncated_provider_data() {
        for s in ["", "AA==", "not base64"] {
            assert!(decode(s, &[0; 16]).is_err());
        }
    }
    #[test]
    fn protocol_key_and_xor_alignment() {
        assert_eq!(xor_derive_key(&[0; 16], 4), vec![0x8d, 0xa5, 0xa5, 0x8d]);
        let mut v = b"1234567".to_vec();
        let old = v.clone();
        xor_transform(&mut v, &[1, 2, 3, 4]);
        xor_transform(&mut v, &[1, 2, 3, 4]);
        assert_eq!(v, old);
    }
}
