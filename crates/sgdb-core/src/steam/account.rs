//! Which Steam account's artwork we are editing.
//!
//! Artwork lives under `userdata/<accountid>/`, so getting this wrong writes art nobody sees.
//!
//! # Resolution order
//!
//! 1. `HKCU\...\Steam\ActiveProcess\ActiveUser` — exact, but **0 when Steam is not running**.
//! 2. `config/loginusers.vdf`, taking the most recent `Timestamp`.
//! 3. If exactly one `userdata/<id>` directory exists, use it.
//!
//! Verified on this machine: `ActiveUser` = `0xf85574` = `16274804`, and
//! `76561197976540532 - 76561197960265728 = 16274804`, matching the `userdata\16274804`
//! folder. `[VERIFIED-BOX 2026-07-27]`
//!
//! Whatever the source, the result is **cross-checked against an existing `userdata/<id>`
//! directory** — a resolved id with no directory is reported rather than used.

use crate::steam::locate::SteamInstall;
use crate::vdf::text;
use std::path::PathBuf;

/// SteamID64 of the first individual account. `accountid = steamid64 - this`.
const STEAMID64_BASE: u64 = 76_561_197_960_265_728;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("no Steam account found (no ActiveUser, no loginusers.vdf, no userdata directories)")]
    NoAccount,

    #[error("several accounts are present; pick one explicitly: {0:?}")]
    Ambiguous(Vec<u32>),

    #[error("reading {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Account {
    pub id: u32,
    pub source: AccountSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountSource {
    ActiveUser,
    LoginUsers,
    OnlyUserdataDir,
}

impl AccountSource {
    pub const fn label(self) -> &'static str {
        match self {
            AccountSource::ActiveUser => r"HKCU\...\ActiveProcess\ActiveUser",
            AccountSource::LoginUsers => "config/loginusers.vdf (most recent)",
            AccountSource::OnlyUserdataDir => "the only userdata directory",
        }
    }
}

/// `accountid` from a SteamID64. Returns `None` for values below the individual-account base.
pub fn account_id_from_steamid64(steamid64: u64) -> Option<u32> {
    steamid64
        .checked_sub(STEAMID64_BASE)
        .and_then(|v| u32::try_from(v).ok())
}

/// Account ids that actually have a `userdata/<id>` directory.
pub fn userdata_accounts(install: &SteamInstall) -> Vec<u32> {
    let Ok(entries) = std::fs::read_dir(install.userdata_dir()) else {
        return Vec::new();
    };
    let mut ids: Vec<u32> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().to_str()?.parse().ok())
        // Steam creates `userdata/0` on some installs; it is not a real account.
        .filter(|&id: &u32| id != 0)
        .collect();
    ids.sort_unstable();
    ids
}

/// Most-recently-logged-in account from `loginusers.vdf`, as an accountid.
pub fn most_recent_login(install: &SteamInstall) -> Result<Option<u32>, Error> {
    let path = install.loginusers_vdf();
    if !path.is_file() {
        return Ok(None);
    }
    let raw = std::fs::read(&path).map_err(|e| Error::Read {
        path: path.clone(),
        source: e,
    })?;
    // Game and persona names can be non-UTF-8; a lossy read must not lose the whole file.
    let doc = match text::parse(&String::from_utf8_lossy(&raw)) {
        Ok(d) => d,
        Err(_) => return Ok(None),
    };

    let Some(users) = text::get(&doc.entries, "users").and_then(|v| v.as_map()) else {
        return Ok(None);
    };

    let mut best: Option<(u64, u32)> = None;
    for entry in users {
        let Ok(steamid64) = entry.key.parse::<u64>() else {
            continue;
        };
        let Some(id) = account_id_from_steamid64(steamid64) else {
            continue;
        };
        let Some(fields) = entry.value.as_map() else {
            continue;
        };

        // Prefer MostRecent when set, else fall back to the newest Timestamp.
        let most_recent = text::get(fields, "MostRecent")
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
            == 1;
        let timestamp = text::get(fields, "Timestamp")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let rank = if most_recent { u64::MAX } else { timestamp };

        if best.is_none_or(|(b, _)| rank > b) {
            best = Some((rank, id));
        }
    }
    Ok(best.map(|(_, id)| id))
}

/// Resolve the account whose artwork we should edit.
pub fn resolve(install: &SteamInstall) -> Result<Account, Error> {
    let present = userdata_accounts(install);

    // 1. The signed-in user, when Steam is running.
    if let Some(id) = crate::steam::locate::active_user()
        && present.contains(&id)
    {
        return Ok(Account {
            id,
            source: AccountSource::ActiveUser,
        });
    }

    // 2. The most recent login.
    if let Some(id) = most_recent_login(install)?
        && present.contains(&id)
    {
        return Ok(Account {
            id,
            source: AccountSource::LoginUsers,
        });
    }

    // 3. An unambiguous single account.
    match present.len() {
        0 => Err(Error::NoAccount),
        1 => Ok(Account {
            id: present[0],
            source: AccountSource::OnlyUserdataDir,
        }),
        // Guessing here would write artwork into the wrong user's profile.
        _ => Err(Error::Ambiguous(present)),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "test assertions are allowed to panic")]
mod tests {
    use super::*;

    fn install_with(users: &[u32], loginusers: Option<&str>) -> (tempfile::TempDir, SteamInstall) {
        let t = tempfile::tempdir().unwrap();
        for id in users {
            std::fs::create_dir_all(t.path().join("userdata").join(id.to_string())).unwrap();
        }
        if let Some(content) = loginusers {
            std::fs::create_dir_all(t.path().join("config")).unwrap();
            // boundary-ok: test fixture written into a tempdir
            std::fs::write(t.path().join("config").join("loginusers.vdf"), content).unwrap();
        }
        let s = SteamInstall::at(t.path());
        (t, s)
    }

    #[test]
    fn the_verified_steamid_conversion() {
        // This machine's account, end to end.
        assert_eq!(
            account_id_from_steamid64(76_561_197_976_540_532),
            Some(16_274_804)
        );
    }

    #[test]
    fn rejects_ids_below_the_individual_base() {
        assert_eq!(account_id_from_steamid64(0), None);
        assert_eq!(account_id_from_steamid64(STEAMID64_BASE - 1), None);
        assert_eq!(account_id_from_steamid64(STEAMID64_BASE), Some(0));
    }

    #[test]
    fn lists_userdata_accounts_and_skips_zero() {
        let (_t, s) = install_with(&[0, 16_274_804, 99], None);
        assert_eq!(userdata_accounts(&s), vec![99, 16_274_804]);
    }

    #[test]
    fn picks_the_most_recent_login() {
        let vdf = r#"
"users"
{
	"76561197976540532"
	{
		"AccountName"		"olduser"
		"Timestamp"		"1000"
	}
	"76561197976540533"
	{
		"AccountName"		"newuser"
		"Timestamp"		"2000"
	}
}
"#;
        let (_t, s) = install_with(&[16_274_804, 16_274_805], Some(vdf));
        assert_eq!(most_recent_login(&s).unwrap(), Some(16_274_805));
    }

    #[test]
    fn most_recent_flag_wins_over_a_newer_timestamp() {
        let vdf = r#"
"users"
{
	"76561197976540532" { "Timestamp" "1000" "MostRecent" "1" }
	"76561197976540533" { "Timestamp" "9999" "MostRecent" "0" }
}
"#;
        let (_t, s) = install_with(&[16_274_804, 16_274_805], Some(vdf));
        assert_eq!(most_recent_login(&s).unwrap(), Some(16_274_804));
    }

    #[test]
    fn resolves_a_single_account_without_any_metadata() {
        let (_t, s) = install_with(&[16_274_804], None);
        let acct = resolve(&s).unwrap();
        assert_eq!(acct.id, 16_274_804);
    }

    #[test]
    fn refuses_to_guess_between_several_accounts() {
        let (_t, s) = install_with(&[111, 222], None);
        // Writing art into the wrong user's profile is worse than asking.
        assert!(matches!(resolve(&s), Err(Error::Ambiguous(ids)) if ids == vec![111, 222]));
    }

    #[test]
    fn an_account_named_in_loginusers_but_absent_on_disk_is_not_used() {
        let vdf = r#""users" { "76561197976540533" { "Timestamp" "2000" } }"#;
        // loginusers names 16274805, but only 16274804 has a directory.
        let (_t, s) = install_with(&[16_274_804], Some(vdf));
        let acct = resolve(&s).unwrap();
        assert_eq!(
            acct.id, 16_274_804,
            "must fall through to the directory that exists"
        );
    }

    #[test]
    fn no_accounts_at_all_is_an_error() {
        let (_t, s) = install_with(&[], None);
        assert!(matches!(resolve(&s), Err(Error::NoAccount)));
    }

    #[test]
    fn a_malformed_loginusers_does_not_abort_resolution() {
        let (_t, s) = install_with(&[16_274_804], Some("{{{ not vdf"));
        assert_eq!(resolve(&s).unwrap().id, 16_274_804);
    }
}
