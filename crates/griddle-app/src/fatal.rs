//! Reporting a startup failure in a binary that has no console and no window yet.
//!
//! **`eprintln!` here reaches nobody.** `windows_subsystem = "windows"` means there is no
//! console attached, so the process simply exits with nothing printed anywhere — the app just
//! does not appear. That is the single worst failure mode a downloaded portable exe can have:
//! double-clicking it does nothing at all, with no message to search for and nothing to report.
//!
//! The realistic cause is a **missing WebView2 runtime**. It ships with Windows 11 and with Edge,
//! so it is present on almost every machine, but "almost every" is not "every" — and the portable
//! zip has no installer to add it. A message box is the only surface left at that point.

use windows_sys::Win32::UI::WindowsAndMessaging::{
    IDYES, MB_ICONERROR, MB_YESNO, MessageBoxW, SW_SHOWNORMAL,
};

/// Where Microsoft publishes the Evergreen bootstrapper.
///
/// Allowlisted separately from `griddle_core::browser`, which only permits steamgriddb.com. This
/// path runs before anything else is up, and hard-codes its one destination rather than taking a
/// string, so there is nothing here that could be pointed somewhere else.
const WEBVIEW2_URL: &str = "https://developer.microsoft.com/microsoft-edge/webview2/";

/// Report a fatal startup failure and exit.
///
/// Offers to open the WebView2 download page, because that is the fix for the only cause anyone
/// is likely to hit. Never returns.
pub fn no_window(error: &dyn std::fmt::Display) -> ! {
    // Kept for a `cargo run` from a terminal, where it does reach someone.
    eprintln!("fatal: could not start the application window: {error}");
    tracing::error!(%error, "could not start the application window");

    let text = format!(
        "Griddle could not start its window.\n\n\
         This usually means the Microsoft Edge WebView2 runtime is missing. It is a free \
         Microsoft component that ships with Windows 11 and with Edge, and Griddle draws its \
         interface with it.\n\n\
         Open the download page?\n\n\
         Technical detail:\n{error}"
    );

    if message_box(&text, "Griddle") == IDYES {
        open_webview2_page();
    }
    std::process::exit(1);
}

fn message_box(text: &str, caption: &str) -> i32 {
    let text = wide(text);
    let caption = wide(caption);
    // SAFETY: both pointers are to NUL-terminated UTF-16 buffers that outlive the call, and a
    // null owner window is valid — there is no window yet, which is the entire situation.
    #[allow(
        unsafe_code,
        reason = "MessageBoxW is the only way to reach a user in a windows_subsystem binary \
                  whose window failed to start"
    )]
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            caption.as_ptr(),
            MB_ICONERROR | MB_YESNO,
        )
    }
}

fn open_webview2_page() {
    let url = wide(WEBVIEW2_URL);
    let verb = wide("open");
    // SAFETY: both pointers are to NUL-terminated UTF-16 buffers that outlive the call. The URL
    // is a compile-time constant, so the shell cannot be handed an arbitrary string here.
    #[allow(
        unsafe_code,
        reason = "ShellExecuteW is how Windows opens a URL in the default browser"
    )]
    let result = unsafe {
        windows_sys::Win32::UI::Shell::ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            url.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // ShellExecuteW returns a value <= 32 to mean failure, which is a genuine Win32 wart. There
    // is nothing useful to do about it here — the user is about to see the process exit either
    // way — but it is logged rather than discarded.
    if (result as isize) <= 32 {
        tracing::warn!("could not open the WebView2 download page");
    }
}

/// A NUL-terminated UTF-16 buffer, which is what every `*W` Win32 entry point wants.
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
