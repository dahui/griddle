//! Tests for [`super`]. Split out to keep the implementation readable on its own.

use super::*;

#[test]
fn enumeration_finds_this_process() {
    // A weak assertion on purpose: the point is that the ToolHelp call returns something
    // rather than silently yielding an empty list, which would make `verify_stopped`
    // hand out tokens on a machine where Steam is running.
    let all = imp::enumerate();
    if cfg!(windows) {
        assert!(!all.is_empty(), "process enumeration returned nothing");
        let me = std::process::id();
        assert!(
            all.iter().any(|p| p.pid == me),
            "our own pid {me} was not in the snapshot"
        );
    }
}

#[test]
fn only_steam_processes_are_matched() {
    // `running()` filters by exact name, so a process merely containing "steam" must not
    // count. Checked against the real snapshot: whatever is running, nothing named e.g.
    // "steamfriends.exe" should be reported.
    for p in running() {
        assert!(
            STEAM_PROCESSES
                .iter()
                .any(|w| p.name.eq_ignore_ascii_case(w)),
            "{} should not have matched",
            p.name
        );
    }
}

#[test]
fn a_synthetic_token_reconfirms_without_touching_the_system() {
    let t = SteamStopped::synthetic_for_test();
    assert!(t.reconfirm().is_ok());
    assert!(t.age() < Duration::from_secs(5));
}

fn sample() -> Vec<SteamProcess> {
    vec![
        SteamProcess {
            pid: 43,
            name: "steamwebhelper.exe".into(),
        },
        SteamProcess {
            pid: 42,
            name: "steam.exe".into(),
        },
    ]
}

#[test]
fn the_running_error_names_the_processes_and_says_what_to_do() {
    let (count, names) = describe(&sample());
    let msg = Error::StillRunning { count, names }.to_string();
    // The message has to be actionable on its own — it is what the UI shows.
    assert!(msg.contains("steam.exe (pid 42)"), "{msg}");
    assert!(msg.contains("steamwebhelper.exe (pid 43)"), "{msg}");
    assert!(msg.contains("Close Steam"), "{msg}");
    // "still running after 0ns" was the first draft. A duration belongs only on the
    // timeout case, where waiting actually happened.
    assert!(!msg.contains("0ns"), "{msg}");
}

#[test]
fn the_timeout_error_is_distinct_and_reports_how_long_it_waited() {
    let (count, names) = describe(&sample());
    let msg = Error::ShutdownTimedOut {
        waited: Duration::from_secs(45),
        count,
        names,
    }
    .to_string();
    assert!(msg.contains("45s"), "{msg}");
    // Different remedy from StillRunning, so it must not tell the user to close Steam —
    // they already did, via us.
    assert!(!msg.contains("Close Steam"), "{msg}");
}

#[test]
fn process_descriptions_are_sorted_for_stable_error_text() {
    let (count, names) = describe(&sample());
    assert_eq!(count, 2);
    assert_eq!(names, "steam.exe (pid 42), steamwebhelper.exe (pid 43)");
}

#[test]
fn verify_stopped_agrees_with_is_running() {
    // Whichever state this machine is in, the two must not disagree — a token handed out
    // while `is_running()` is true would be exactly the bug this module prevents.
    let token = verify_stopped();
    assert_eq!(token.is_ok(), !is_running());
}

#[cfg(not(windows))]
#[test]
fn spawning_is_refused_off_windows() {
    assert!(matches!(
        imp::spawn_detached(std::path::Path::new("/steam"), &[]),
        Err(Error::UnsupportedPlatform)
    ));
}
