//! The error shape that crosses into the UI.
//!
//! `griddle-core` uses `thiserror` precisely so failures stay distinguishable, and that only pays
//! off if the distinction survives the boundary. A single `String` here would collapse
//! "Steam is running, close it" and "your network timed out" into the same red toast, and the
//! user would have no idea which of the two very different actions to take.
//!
//! So every error carries three things:
//!
//! - a machine-readable [`Kind`] the frontend can branch on,
//! - a sentence for the user,
//! - and, where one exists, the **action** that would fix it.
//!
//! `action` is not decoration. Most failures in this app are environmental — Steam not
//! running, a key not yet entered, a port occupied — and for those, what to do next is the
//! only genuinely useful part of the message.

use serde::Serialize;

/// What went wrong, in a form the UI can switch on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// No API key stored yet — the first-run state, not really a failure.
    NoApiKey,
    /// SteamGridDB rejected the key.
    Unauthorized,
    /// Network trouble reaching SteamGridDB.
    Network,
    /// SteamGridDB has no entry for this game.
    NotOnSteamGridDb,
    /// Steam is not installed, or could not be located.
    SteamNotFound,
    /// Steam is running and the operation needs it stopped.
    SteamRunning,
    /// Live apply is unavailable; the file-write path was used or is needed.
    LiveApplyUnavailable,
    /// Writing to disk failed.
    Filesystem,
    /// Anything we have not classified. Deliberately last.
    Unexpected,
}

#[derive(Debug, Clone, Serialize)]
pub struct UiError {
    pub kind: Kind,
    pub message: String,
    /// What the user can do about it. `None` when there is genuinely nothing.
    pub action: Option<String>,
}

impl UiError {
    pub fn new(kind: Kind, message: impl Into<String>) -> Self {
        UiError {
            kind,
            message: message.into(),
            action: None,
        }
    }

    pub fn with_action(mut self, action: impl Into<String>) -> Self {
        self.action = Some(action.into());
        self
    }

    pub fn no_api_key() -> Self {
        UiError::new(Kind::NoApiKey, "No SteamGridDB API key yet.")
            .with_action("Add yours in Settings to start browsing.")
    }

    pub fn steam_not_found(detail: impl Into<String>) -> Self {
        UiError::new(Kind::SteamNotFound, detail)
            .with_action("Check that Steam is installed, or set SGDB_STEAM_PATH.")
    }

    pub fn unexpected(detail: impl std::fmt::Display) -> Self {
        UiError::new(Kind::Unexpected, detail.to_string())
    }
}

impl std::fmt::Display for UiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for UiError {}

// -- conversions from the core error types --------------------------------------------------
//
// Each of these is where a core error's *meaning* is preserved rather than flattened. When a
// new variant is added to a core error, the compiler does not force an update here — so the
// catch-all arms deliberately produce `Unexpected` rather than guessing.

impl From<griddle_core::sgdb::client::Error> for UiError {
    fn from(e: griddle_core::sgdb::client::Error) -> Self {
        use griddle_core::sgdb::client::Error as E;
        match &e {
            // No screen is named: this same action is read on the first-run screen, where there
            // is no Settings tab yet, and inside Settings, where saying so is redundant. What
            // does not change in either place is where a replacement key comes from.
            E::Unauthorized => UiError::new(Kind::Unauthorized, e.to_string()).with_action(
                "Generate a fresh key on your SteamGridDB preferences page, then paste it again.",
            ),
            E::NotFound => UiError::new(Kind::NotOnSteamGridDb, e.to_string())
                .with_action("Try “Wrong game?” to search for it by name."),
            E::Timeout | E::Network(_) => UiError::new(Kind::Network, e.to_string())
                .with_action("Check your connection and try again."),
            E::RateLimited => UiError::new(Kind::Network, e.to_string())
                .with_action("SteamGridDB is busy. Wait a moment and try again."),
            _ => UiError::unexpected(e),
        }
    }
}

impl From<griddle_core::grid::store::Error> for UiError {
    fn from(e: griddle_core::grid::store::Error) -> Self {
        UiError::new(Kind::Filesystem, e.to_string())
            .with_action("Check that Steam's userdata folder is writable.")
    }
}

/// What to tell the user about a Steam process that is in the way.
///
/// Shared with the `shortcuts::Error` conversion below, which wraps this same error: a shortcut
/// write that is refused because Steam came back deserves the same wording as one refused before
/// it started. `None` for the variants that are not about Steam being up.
fn steam_running_action(e: &griddle_core::steam::process::Error) -> Option<&'static str> {
    use griddle_core::steam::process::Error as E;
    match e {
        E::StillRunning { .. } | E::Restarted => Some("Close Steam, then retry."),
        E::ShutdownTimedOut { .. } => {
            Some("Steam may be waiting on a prompt. Close it by hand, then retry.")
        }
        _ => None,
    }
}

impl From<griddle_core::steam::process::Error> for UiError {
    fn from(e: griddle_core::steam::process::Error) -> Self {
        match steam_running_action(&e) {
            Some(action) => UiError::new(Kind::SteamRunning, e.to_string()).with_action(action),
            None => UiError::unexpected(e),
        }
    }
}

impl From<griddle_core::steam::shortcuts::Error> for UiError {
    fn from(e: griddle_core::steam::shortcuts::Error) -> Self {
        use griddle_core::steam::shortcuts::Error as E;
        match &e {
            // This arrives when `save` re-confirms its token and finds Steam back — the window
            // between checking and writing that the token type alone cannot close. The action
            // comes from the same helper the process conversion uses, so the two cannot drift.
            E::SteamRunning(inner) => UiError::new(Kind::SteamRunning, e.to_string())
                .with_action(steam_running_action(inner).unwrap_or("Close Steam, then retry.")),

            E::NotFound(..) => UiError::new(Kind::Unexpected, e.to_string())
                .with_action("Steam may have rewritten shortcuts.vdf. Reopen the game and retry."),

            // The refusals, kept distinct because they mean the file was *not* touched and the
            // user's shortcuts are intact — which is the first thing anyone wants to know.
            E::RoundTripMismatch { .. } | E::SelfCheckFailed | E::NoShortcutsMap { .. } => {
                UiError::new(Kind::Filesystem, e.to_string()).with_action(
                    "Griddle refused to write rather than risk corrupting your shortcuts. \
                     Nothing was changed.",
                )
            }

            E::Read { .. } | E::Write { .. } | E::Parse { .. } | E::VerifyFailed { .. } => {
                UiError::new(Kind::Filesystem, e.to_string())
            }
        }
    }
}

impl From<griddle_core::settings::Error> for UiError {
    fn from(e: griddle_core::settings::Error) -> Self {
        UiError::new(Kind::Filesystem, e.to_string())
    }
}

impl From<griddle_core::cdp::Error> for UiError {
    fn from(e: griddle_core::cdp::Error) -> Self {
        UiError::new(Kind::LiveApplyUnavailable, e.to_string())
            .with_action("Artwork will be written to disk instead, so Steam needs a restart.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_failures_the_plan_names_stay_distinguishable() {
        // "Steam is running, can't write shortcuts" vs "network timeout" — the exact pair the
        // architecture exists to keep apart. If these ever collapse to one kind, the UI cannot
        // tell the user which of two unrelated actions to take.
        let running: UiError = griddle_core::steam::process::Error::StillRunning {
            count: 1,
            names: "steam.exe (pid 1)".into(),
        }
        .into();
        let timeout: UiError = griddle_core::sgdb::client::Error::Timeout.into();

        assert_eq!(running.kind, Kind::SteamRunning);
        assert_eq!(timeout.kind, Kind::Network);
        assert_ne!(running.kind, timeout.kind);
        assert!(running.action.unwrap().contains("Close Steam"));
        assert!(timeout.action.unwrap().contains("connection"));
    }

    #[test]
    fn a_bad_key_is_not_reported_as_a_network_problem() {
        let e: UiError = griddle_core::sgdb::client::Error::Unauthorized.into();
        assert_eq!(e.kind, Kind::Unauthorized);
        let action = e.action.unwrap();
        // Says where a key comes from, not which screen to be on. The first-run screen shows
        // this before a Settings tab exists, so naming one sends the user nowhere.
        assert!(action.contains("preferences page"), "{action}");
        assert!(!action.contains("Settings"), "{action}");
    }

    #[test]
    fn a_game_missing_from_steamgriddb_is_its_own_kind() {
        // Not an error the user caused, and not one retrying fixes.
        let e: UiError = griddle_core::sgdb::client::Error::NotFound.into();
        assert_eq!(e.kind, Kind::NotOnSteamGridDb);
    }

    #[test]
    fn every_environmental_failure_carries_an_action() {
        // These are the ones where "what do I do now" is the only useful part of the message.
        for e in [
            UiError::no_api_key(),
            UiError::steam_not_found("nope"),
            griddle_core::sgdb::client::Error::Unauthorized.into(),
            griddle_core::sgdb::client::Error::Timeout.into(),
        ] {
            assert!(e.action.is_some(), "{:?} needs an action", e.kind);
        }
    }

    #[test]
    fn errors_serialise_with_a_snake_case_kind_the_frontend_can_switch_on() {
        let json = serde_json::to_value(UiError::no_api_key()).unwrap();
        assert_eq!(json["kind"], "no_api_key");
        assert!(json["message"].is_string());
        assert!(json["action"].is_string());
    }

    #[test]
    fn an_unclassified_error_is_unexpected_rather_than_mislabelled() {
        // Guessing would be worse than admitting we do not know: a wrong action sends the user
        // off to fix something that is not broken.
        let e: UiError = griddle_core::sgdb::client::Error::Decode("bad json".into()).into();
        assert_eq!(e.kind, Kind::Unexpected);
        assert!(e.action.is_none());
    }
}
