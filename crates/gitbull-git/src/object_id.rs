//! Object names as compact values.

use std::fmt;

/// The name of a Git object: 20 bytes for SHA-1, 32 for SHA-256.
///
/// Fixed size and `Copy`, so that millions of them fit in plain arrays.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjectId {
    bytes: [u8; 32],
    len: u8,
}

impl ObjectId {
    /// Reads a full object name in hexadecimal, 40 or 64 digits.
    pub fn from_hex(hex: &[u8]) -> Option<ObjectId> {
        if hex.len() != 40 && hex.len() != 64 {
            return None;
        }
        let mut bytes = [0; 32];
        for (byte, &[high, low]) in bytes.iter_mut().zip(hex.as_chunks::<2>().0) {
            *byte = digit(high)? << 4 | digit(low)?;
        }
        Some(ObjectId {
            bytes,
            len: (hex.len() / 2) as u8,
        })
    }

    /// Takes a name from its raw bytes, 20 or 32 of them.
    pub fn from_bytes(raw: &[u8]) -> Option<ObjectId> {
        if raw.len() != 20 && raw.len() != 32 {
            return None;
        }
        let mut bytes = [0; 32];
        bytes[..raw.len()].copy_from_slice(raw);
        Some(ObjectId {
            bytes,
            len: raw.len() as u8,
        })
    }

    /// The raw bytes of the name.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }

    /// The first `digits` hexadecimal digits, as shown in lists.
    pub fn short(&self, digits: usize) -> String {
        let mut hex = self.to_string();
        hex.truncate(digits);
        hex
    }
}

fn digit(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

impl fmt::Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.as_bytes() {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ObjectId({self})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA1: &str = "0123456789abcdef0123456789abcdef01234567";
    const SHA256: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn sha1_name_round_trips() {
        let id = ObjectId::from_hex(SHA1.as_bytes()).unwrap();
        assert_eq!(id.to_string(), SHA1);
        assert_eq!(id.as_bytes().len(), 20);
        assert_eq!(id.as_bytes()[..3], [0x01, 0x23, 0x45]);
    }

    #[test]
    fn sha256_name_round_trips() {
        let id = ObjectId::from_hex(SHA256.as_bytes()).unwrap();
        assert_eq!(id.to_string(), SHA256);
        assert_eq!(id.as_bytes().len(), 32);
    }

    #[test]
    fn upper_case_digits_are_read() {
        let upper = SHA1.to_uppercase();
        assert_eq!(
            ObjectId::from_hex(upper.as_bytes()).unwrap().to_string(),
            SHA1
        );
    }

    #[test]
    fn names_of_other_lengths_or_with_other_characters_are_rejected() {
        assert_eq!(ObjectId::from_hex(b"0123"), None);
        assert_eq!(ObjectId::from_hex(&SHA1.as_bytes()[..39]), None);
        let mut bad = SHA1.to_owned();
        bad.replace_range(0..1, "g");
        assert_eq!(ObjectId::from_hex(bad.as_bytes()), None);
    }

    #[test]
    fn raw_bytes_round_trip() {
        let id = ObjectId::from_hex(SHA256.as_bytes()).unwrap();
        assert_eq!(ObjectId::from_bytes(id.as_bytes()), Some(id));
        let sha1 = ObjectId::from_hex(SHA1.as_bytes()).unwrap();
        assert_eq!(ObjectId::from_bytes(sha1.as_bytes()), Some(sha1));
        assert_eq!(ObjectId::from_bytes(&[0; 21]), None);
    }

    #[test]
    fn short_form_is_the_leading_digits() {
        let id = ObjectId::from_hex(SHA1.as_bytes()).unwrap();
        assert_eq!(id.short(7), "0123456");
    }

    #[test]
    fn names_of_different_lengths_differ_even_with_equal_leading_bytes() {
        let short = ObjectId::from_hex(SHA1.as_bytes()).unwrap();
        let long =
            ObjectId::from_hex(format!("{SHA1}0123456789abcdef01234567").as_bytes()).unwrap();
        assert_ne!(short, long);
    }
}
