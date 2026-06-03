//! GP2 JAM cipher: symmetric streaming XOR, key 0xB082F165, ×5 keystream.
//! Mirrors JamDecrypt @ 0x70D34. Applying it twice is the identity.

/// Decrypt/encrypt in place. Symmetric.
pub fn jam_xor(buf: &mut [u8]) {
    let mut key: u32 = 0xB082_F164 | 1; // 0xB082F165
    let n = buf.len() / 4;
    for i in 0..n {
        let o = i * 4;
        let mut d = u32::from_le_bytes([buf[o], buf[o + 1], buf[o + 2], buf[o + 3]]);
        d ^= key;
        buf[o..o + 4].copy_from_slice(&d.to_le_bytes());
        key = key.wrapping_mul(5);
    }
    let rem = buf.len() & 3;
    let mut k = key;
    for i in 0..rem {
        buf[n * 4 + i] ^= (k & 0xFF) as u8;
        k >>= 8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jam_is_symmetric() {
        let original: Vec<u8> = (0u8..=200).collect();
        let mut buf = original.clone();
        jam_xor(&mut buf);
        assert_ne!(buf, original, "encryption must change the bytes");
        jam_xor(&mut buf);
        assert_eq!(buf, original, "applying twice must be identity");
    }

    #[test]
    fn jam_first_dword_uses_initial_key() {
        let mut buf = vec![0u8; 8];
        jam_xor(&mut buf);
        assert_eq!(&buf[0..4], &0xB082_F165u32.to_le_bytes());
        assert_eq!(&buf[4..8], &0xB082_F165u32.wrapping_mul(5).to_le_bytes());
    }
}
