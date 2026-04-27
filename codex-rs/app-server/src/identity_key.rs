use std::ffi::OsString;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdentityKey {
    bytes: Vec<u8>,
}

impl IdentityKey {
    pub fn from_bytes(bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            bytes: bytes.into(),
        }
    }

    pub fn from_os_string(value: OsString) -> Self {
        Self::from_bytes(os_string_to_bytes(value))
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

#[cfg(unix)]
fn os_string_to_bytes(value: OsString) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;

    value.as_os_str().as_bytes().to_vec()
}

#[cfg(not(unix))]
fn os_string_to_bytes(value: OsString) -> Vec<u8> {
    value.to_string_lossy().into_owned().into_bytes()
}

#[cfg(test)]
mod tests {
    use super::IdentityKey;
    use pretty_assertions::assert_eq;

    #[test]
    fn identity_key_preserves_opaque_bytes() {
        let key = IdentityKey::from_bytes(b"tenant-key-\x00\xff".to_vec());
        assert_eq!(key.as_bytes(), &b"tenant-key-\x00\xff"[..]);
    }

    #[cfg(unix)]
    #[test]
    fn identity_key_preserves_unix_argv_bytes() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let key = IdentityKey::from_os_string(OsString::from_vec(b"tenant-key-\xff".to_vec()));

        assert_eq!(key.as_bytes(), &b"tenant-key-\xff"[..]);
    }
}
