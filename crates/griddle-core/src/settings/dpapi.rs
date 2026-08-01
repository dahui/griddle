//! Windows DPAPI — encrypting the API key at rest.
//!
//! `CryptProtectData` ties the ciphertext to the **current user account**, so a settings file
//! copied to another machine or read by another user is useless. That is the right threat
//! model here: this protects against a key being scraped out of a plaintext config by anything
//! that can read the user's files, not against the user themselves.
//!
//! # Why an entropy value
//!
//! The optional second parameter means another application on the same machine cannot simply
//! call `CryptUnprotectData` on our blob and get the key back — it has to know [`ENTROPY`]
//! too. That is a speed bump rather than a wall (the value is in this source file, which
//! ships), but it costs nothing and it stops the most casual case.
//!
//! # There is no non-Windows fallback, deliberately
//!
//! Off Windows these functions return [`Error::UnsupportedPlatform`]. The tempting alternative
//! — writing the key in plaintext with a warning — would mean a build where the secret is
//! silently unprotected, and a warning nobody reads. The crate compiles on Linux for CI only;
//! the product is Windows-only.

/// Application-specific secondary entropy. Changing this **invalidates every stored key**, so
/// treat it as a format version: bump it only with a migration path.
///
/// It read `sgdb-core:api-key:v1` before the product had a name. Changing it cost one re-entry
/// of the key and nothing else, because nothing had shipped — the only moment that is ever free.
///
/// Its only reader is `imp`, which is Windows-only, so off Windows it is dead by construction.
/// Kept rather than `cfg`-gated so the module documentation above can still link to it.
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "read only by the Windows-only `imp` module")
)]
const ENTROPY: &[u8] = b"griddle:api-key:v1";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Windows could not encrypt the value (DPAPI error {code})")]
    Protect { code: u32 },

    #[error(
        "Windows could not decrypt the stored API key (DPAPI error {code}). \
         It was most likely saved by a different Windows user account."
    )]
    Unprotect { code: u32 },

    #[error("DPAPI is only available on Windows")]
    UnsupportedPlatform,
}

/// Encrypt for the current user.
pub fn protect(plaintext: &[u8]) -> Result<Vec<u8>, Error> {
    imp::protect(plaintext)
}

/// Decrypt something [`protect`] produced.
pub fn unprotect(ciphertext: &[u8]) -> Result<Vec<u8>, Error> {
    imp::unprotect(ciphertext)
}

#[cfg(windows)]
mod imp {
    use super::{ENTROPY, Error};

    /// Never show a UI prompt. This can be called from a background task, and a modal Windows
    /// dialog appearing out of nowhere would be both baffling and a hang.
    const CRYPTPROTECT_UI_FORBIDDEN: u32 = 0x1;

    #[allow(
        unsafe_code,
        reason = "DPAPI is a raw FFI surface with no safe wrapper in std; the unsafe is \
                  confined to this function and every allocation is freed on the way out"
    )]
    fn call(protecting: bool, input: &[u8]) -> Result<Vec<u8>, Error> {
        // `LocalFree` is under Foundation, not System::Memory — DPAPI hands back a
        // LocalAlloc'd buffer that we own and must release.
        use windows_sys::Win32::Foundation::{GetLastError, LocalFree};
        use windows_sys::Win32::Security::Cryptography::{
            CRYPT_INTEGER_BLOB, CryptProtectData, CryptUnprotectData,
        };

        let in_blob = CRYPT_INTEGER_BLOB {
            cbData: input.len() as u32,
            pbData: input.as_ptr().cast_mut(),
        };
        let entropy_blob = CRYPT_INTEGER_BLOB {
            cbData: ENTROPY.len() as u32,
            pbData: ENTROPY.as_ptr().cast_mut(),
        };
        let mut out = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };

        // SAFETY: both input blobs point at live slices that outlive the call and are only
        // read. `out` is written by the API and freed below on every success path.
        let ok = unsafe {
            if protecting {
                CryptProtectData(
                    &in_blob,
                    std::ptr::null(),
                    &entropy_blob,
                    std::ptr::null(),
                    std::ptr::null(),
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut out,
                )
            } else {
                CryptUnprotectData(
                    &in_blob,
                    std::ptr::null_mut(),
                    &entropy_blob,
                    std::ptr::null(),
                    std::ptr::null(),
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut out,
                )
            }
        };

        if ok == 0 {
            // SAFETY: no pointer dereference; reads the calling thread's last error.
            let code = unsafe { GetLastError() };
            return Err(if protecting {
                Error::Protect { code }
            } else {
                Error::Unprotect { code }
            });
        }

        // SAFETY: the call succeeded, so `pbData` points at `cbData` initialised bytes owned
        // by LocalAlloc. Copied out, then freed immediately.
        let result = unsafe {
            let slice = std::slice::from_raw_parts(out.pbData, out.cbData as usize);
            let copied = slice.to_vec();
            // Wipe the API's buffer before releasing it, so a decrypted key does not linger in
            // freed heap memory any longer than necessary.
            std::ptr::write_bytes(out.pbData, 0, out.cbData as usize);
            LocalFree(out.pbData.cast());
            copied
        };

        Ok(result)
    }

    pub fn protect(plaintext: &[u8]) -> Result<Vec<u8>, Error> {
        call(true, plaintext)
    }

    pub fn unprotect(ciphertext: &[u8]) -> Result<Vec<u8>, Error> {
        call(false, ciphertext)
    }
}

#[cfg(not(windows))]
mod imp {
    use super::Error;

    pub fn protect(_plaintext: &[u8]) -> Result<Vec<u8>, Error> {
        Err(Error::UnsupportedPlatform)
    }

    pub fn unprotect(_ciphertext: &[u8]) -> Result<Vec<u8>, Error> {
        Err(Error::UnsupportedPlatform)
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    const SECRET: &[u8] = b"0123456789abcdef0123456789abcdef";

    #[test]
    fn round_trips_a_key() {
        let sealed = protect(SECRET).unwrap();
        assert_eq!(unprotect(&sealed).unwrap(), SECRET);
    }

    #[test]
    fn the_ciphertext_does_not_contain_the_plaintext() {
        // The whole point. If DPAPI were somehow a no-op this would catch it.
        let sealed = protect(SECRET).unwrap();
        assert!(
            sealed.windows(SECRET.len()).all(|w| w != SECRET),
            "the plaintext key is present in the ciphertext"
        );
        assert!(sealed.len() > SECRET.len(), "expected a wrapped blob");
    }

    #[test]
    fn two_encryptions_of_the_same_value_differ() {
        // DPAPI salts each call, so identical keys must not produce identical blobs — which
        // would otherwise let someone compare two settings files and learn they match.
        assert_ne!(protect(SECRET).unwrap(), protect(SECRET).unwrap());
    }

    #[test]
    fn a_corrupted_blob_fails_rather_than_returning_garbage() {
        let mut sealed = protect(SECRET).unwrap();
        let last = sealed.len() - 1;
        sealed[last] ^= 0xFF;
        assert!(unprotect(&sealed).is_err());
    }

    #[test]
    fn an_empty_value_round_trips() {
        assert_eq!(unprotect(&protect(b"").unwrap()).unwrap(), b"");
    }
}
