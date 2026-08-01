//! App identifiers.
//!
//! # Why these are newtypes
//!
//! A non-Steam shortcut's id appears in **two forms**, and mixing them writes artwork to a
//! filename Steam never reads:
//!
//! | Form | Where |
//! |---|---|
//! | signed `i32` | the `appid` field inside `shortcuts.vdf` |
//! | unsigned `u32` | artwork filenames, and the JS/CDP APIs |
//!
//! On this machine the same shortcut is `0xF1548865` = `-246118299` signed = `4048848997`
//! unsigned, and its art really is at `grid/4048848997p.png`.
//! `[VERIFIED-BOX 2026-07-27]` `appStore.GetAppOverviewByAppID(4048848997)` also resolves —
//! so the **unsigned** form is what crosses the CDP boundary too.
//!
//! # There is deliberately no CRC32 here
//!
//! Every tutorial says a shortcut's appid is `crc32_ieee(exe + appname) | 0x80000000`. It is
//! **false** on modern Steam — four variants were computed against the real file and none
//! matched; Steam assigns a random high-bit-set value.
//! `[VERIFIED-BOX 2026-07-27]`, corroborated by ValveSoftware/steam-for-linux#9463.
//!
//! **Always read `appid` from `shortcuts.vdf`.** The way to guarantee we never regress to the
//! folklore is for the function not to exist — so do not add one.

use std::fmt;

/// The form used for artwork filenames and the CDP/JS APIs.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AppId(u32);

impl AppId {
    pub const fn new(v: u32) -> Self {
        AppId(v)
    }

    /// From the signed value stored in `shortcuts.vdf`.
    pub const fn from_signed(v: i32) -> Self {
        AppId(v as u32)
    }

    pub const fn get(self) -> u32 {
        self.0
    }

    /// The signed form, for writing back into `shortcuts.vdf`.
    pub const fn to_signed(self) -> i32 {
        self.0 as i32
    }

    /// True for the high-bit-set range Steam assigns to non-Steam shortcuts.
    ///
    /// A useful sanity check, **not** an identity test: use the presence of an entry in
    /// `shortcuts.vdf` to decide what something is.
    pub const fn is_shortcut_range(self) -> bool {
        self.0 & 0x8000_0000 != 0
    }

    /// The 64-bit "BPID" used by `steam://rungameid/` and by older clients' grid filenames.
    ///
    /// Only needed when scanning for *legacy* artwork; never for writing.
    pub const fn to_bpid(self) -> u64 {
        ((self.0 as u64) << 32) | 0x0200_0000
    }
}

impl fmt::Display for AppId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Debug for AppId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_shortcut_range() {
            write!(
                f,
                "AppId({} /* shortcut, signed {} */)",
                self.0,
                self.to_signed()
            )
        } else {
            write!(f, "AppId({})", self.0)
        }
    }
}

impl From<u32> for AppId {
    fn from(v: u32) -> Self {
        AppId(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real shortcut on this machine, in all three forms.
    #[test]
    fn the_verified_shortcut_converts_all_ways() {
        let from_file = AppId::from_signed(-246_118_299); // as stored in shortcuts.vdf
        assert_eq!(from_file.get(), 4_048_848_997); // as used in grid/ filenames and CDP
        assert_eq!(from_file.to_signed(), -246_118_299); // and back
        assert!(from_file.is_shortcut_range());
        assert_eq!(format!("{from_file}"), "4048848997");
    }

    #[test]
    fn a_real_steam_appid_is_not_in_the_shortcut_range() {
        assert!(!AppId::new(1_004_640).is_shortcut_range());
        assert!(!AppId::new(620).is_shortcut_range());
    }

    #[test]
    fn signed_round_trip_is_lossless_across_the_range() {
        for v in [
            0u32,
            1,
            620,
            1_004_640,
            0x7FFF_FFFF,
            0x8000_0000,
            0xF154_8865,
            u32::MAX,
        ] {
            assert_eq!(AppId::from_signed(AppId::new(v).to_signed()).get(), v);
        }
    }

    /// `(appid << 32) | 0x02000000` — used by `steam://rungameid` and by older clients'
    /// grid filenames. Checked by decomposition rather than against a magic literal, so the
    /// test states the rule instead of restating the implementation.
    #[test]
    fn bpid_is_the_appid_in_the_high_word_with_the_marker_low() {
        for id in [4_048_848_997u32, 620, 1_004_640] {
            let b = AppId::new(id).to_bpid();
            assert_eq!(b >> 32, u64::from(id), "high word must be the appid");
            assert_eq!(b & 0xFFFF_FFFF, 0x0200_0000, "low word must be the marker");
        }
    }
}
