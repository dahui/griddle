//! Opening a link in the user's default browser.
//!
//! A Tauri webview has no browser chrome and does not honour `target="_blank"` — an ordinary
//! `<a>` simply does nothing, which is what made the API-key link look broken. Handing the URL
//! to the OS is the only way, and it has to go through the backend because the webview cannot
//! launch anything itself.
//!
//! # 🔴 The allowlist is the point
//!
//! "Open this URL" is a real capability: it hands a string to the shell, which will launch
//! whatever handler is registered for it. A frontend bug — or remote content, since this webview
//! renders images from SteamGridDB — must not be able to turn that into "launch anything".
//!
//! So this refuses everything except **https** on **steamgriddb.com and its subdomains**, which
//! is the only place the app has any reason to send someone. Widening it is a deliberate edit
//! here, not something a caller can do by passing a different string.
//!
//! Note what the scheme check alone buys: without it, `file:///C:/...` would open Explorer and
//! a `.exe` path would run a program.

/// Hosts this app will open. Subdomains are allowed; anything else is not.
const ALLOWED_HOSTS: [&str; 1] = ["steamgriddb.com"];

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("{0:?} is not a URL")]
    NotAUrl(String),

    #[error("only https links can be opened, not {scheme:?}")]
    BadScheme { scheme: String },

    #[error("this app does not open links to {host}")]
    HostNotAllowed { host: String },

    #[error("the system could not open the link (error {code})")]
    ShellFailed { code: isize },

    #[error("opening links is only implemented on Windows")]
    Unsupported,
}

/// Check a URL against the allowlist, returning it unchanged if it passes.
///
/// Separate from [`open`] so the policy is testable without launching a browser on a build
/// machine — the decision is the part worth testing, not the syscall.
pub fn allowed(url: &str) -> Result<&str, Error> {
    let parsed = url::Url::parse(url).map_err(|_| Error::NotAUrl(url.to_owned()))?;

    // Checked before the host, because a `file:` or `javascript:` URL is the dangerous case and
    // the message should say so rather than complaining about a missing host.
    if parsed.scheme() != "https" {
        return Err(Error::BadScheme {
            scheme: parsed.scheme().to_owned(),
        });
    }

    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    let ok = ALLOWED_HOSTS.iter().any(|allowed| {
        // `ends_with` alone would accept `evilsteamgriddb.com`, so a subdomain match has to
        // include the separating dot.
        host == *allowed || host.ends_with(&format!(".{allowed}"))
    });
    if !ok {
        return Err(Error::HostNotAllowed { host });
    }
    Ok(url)
}

/// Open an allowlisted URL in the user's default browser.
pub fn open(url: &str) -> Result<(), Error> {
    let url = allowed(url)?;
    tracing::info!(url, "opening a link in the default browser");
    open_native(url)
}

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "ShellExecuteW is a raw FFI surface with no safe wrapper in std; the unsafe is one \
              call taking pointers to locals that outlive it, and the URL is allowlisted first"
)]
fn open_native(url: &str) -> Result<(), Error> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    /// `ShellExecuteW` returns an `HINSTANCE` for historical reasons; anything at or below 32
    /// is an error code, not a handle.
    const SHELL_SUCCESS_FLOOR: isize = 32;

    let verb: Vec<u16> = "open\0".encode_utf16().collect();
    let target: Vec<u16> = url.encode_utf16().chain(std::iter::once(0)).collect();

    // SAFETY: both strings are NUL-terminated UTF-16 and outlive the call; the remaining
    // pointers are the documented "not applicable" nulls. The URL has already been checked
    // against the allowlist above.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            target.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };

    let code = result as isize;
    if code <= SHELL_SUCCESS_FLOOR {
        return Err(Error::ShellFailed { code });
    }
    Ok(())
}

#[cfg(not(windows))]
fn open_native(_url: &str) -> Result<(), Error> {
    // The product is Windows-only; this exists so the crate still builds on the Linux CI leg,
    // which is what catches cfg-gated code that only compiles on one platform.
    Err(Error::Unsupported)
}

impl Error {
    /// A short note on what the user can do, for the ones where anything can be.
    pub fn suggestion(&self) -> Option<&'static str> {
        match self {
            Error::ShellFailed { .. } | Error::Unsupported => {
                Some("Copy the address into your browser instead.")
            }
            _ => None,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "test assertions are allowed to panic")]
mod tests {
    use super::*;

    #[test]
    fn the_key_page_is_allowed() {
        // The control. Every other case here is a rejection, and a suite of only rejections
        // passes just as well against a function that refuses everything.
        let url = "https://www.steamgriddb.com/profile/preferences/api";
        assert_eq!(allowed(url), Ok(url));
        assert_eq!(
            allowed("https://steamgriddb.com/"),
            Ok("https://steamgriddb.com/")
        );
        assert!(allowed("https://cdn2.steamgriddb.com/thumb/a.jpg").is_ok());
    }

    #[test]
    fn a_lookalike_host_is_refused() {
        // 🔴 The one a naive `ends_with` gets wrong. Without the leading dot,
        // `evilsteamgriddb.com` matches the suffix and would be opened.
        assert_eq!(
            allowed("https://evilsteamgriddb.com/"),
            Err(Error::HostNotAllowed {
                host: "evilsteamgriddb.com".to_owned()
            }),
        );
        assert!(allowed("https://steamgriddb.com.evil.test/").is_err());
    }

    #[test]
    fn schemes_that_would_launch_something_are_refused() {
        // These are the reason the allowlist exists at all: handing `file:` to the shell opens
        // Explorer, and an executable path would run it.
        // Note the absence of a `steam://` example: that literal is banned repo-wide by
        // `scripts/check-boundaries.sh`, which flagged this test when it had one. A custom
        // scheme is covered by `ms-settings:` instead — the property is the same.
        for bad in [
            "file:///C:/Windows/System32/cmd.exe",
            "javascript:alert(1)",
            "http://www.steamgriddb.com/",
            "ms-settings:windowsupdate",
        ] {
            assert!(allowed(bad).is_err(), "{bad} must be refused");
        }
    }

    #[test]
    fn http_is_refused_even_on_an_allowed_host() {
        // Downgrade to plaintext on a host we do trust — the host check alone would pass this.
        assert_eq!(
            allowed("http://www.steamgriddb.com/"),
            Err(Error::BadScheme {
                scheme: "http".to_owned()
            }),
        );
    }

    #[test]
    fn a_host_is_matched_case_insensitively() {
        assert!(allowed("https://WWW.SteamGridDB.com/").is_ok());
    }

    /// The one thing the allowlist tests cannot cover: that the FFI call actually launches
    /// something. Ignored by default because it opens a real browser window.
    ///
    /// ```powershell
    /// cargo test -p griddle-core browser::tests::really_opens -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "opens a real browser window"]
    fn really_opens() {
        open("https://www.steamgriddb.com/profile/preferences/api").unwrap();
    }

    #[test]
    fn nonsense_is_not_a_url() {
        assert!(matches!(allowed("not a url"), Err(Error::NotAUrl(_))));
        assert!(allowed("").is_err());
    }
}
