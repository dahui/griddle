//! Print controller actions as they arrive. Read-only; touches nothing.
//!
//! ```powershell
//! cargo run -p griddle-core --example pad_probe
//! ```
//!
//! What it is for: separating "the pad is not being read" from "the UI is not responding to it".
//! Those look identical in the app and have completely different causes — a disconnected or
//! unrecognised controller versus a focus-model bug.
//!
//! Worth running **twice**: once with Steam closed, and once with Griddle launched from Big
//! Picture so the Steam Overlay is attached and Steam Input is remapping the pad. The second is
//! the case that matters, because it is the one
//! [WebView2Feedback #5507](https://github.com/MicrosoftEdge/WebView2Feedback/issues/5507) breaks
//! for the web Gamepad API and that reading natively is chosen to survive.

use griddle_core::input::{self, FocusGate};
use std::sync::mpsc;

fn main() {
    println!("== controller probe ==");

    // Asked first, and printed whether or not anything is found: an empty list is the single most
    // useful thing this harness can report, and it is the answer that a silent "nothing happens"
    // run leaves ambiguous.
    let pads = input::connected();
    if pads.is_empty() {
        println!("  no controller found — nothing below will ever print.");
        println!("  If one is plugged in, Steam Input may be holding it exclusively.");
    } else {
        for (i, name) in pads.iter().enumerate() {
            println!("  [{i}] {name}");
        }
    }

    println!("\nPush the stick or d-pad; A / B / Y are Accept / Back / Menu.");
    println!("Ctrl-C to stop.\n");

    let (tx, rx) = mpsc::channel();
    // Always open: this harness has no window, so there is nothing to gate on. The app gates on
    // window focus, because it reads the pad globally and must not drive the UI in the background.
    let _thread = input::spawn(FocusGate::new(true), move |action| {
        if tx.send(action).is_err() {
            // The main loop has exited, so there is nothing left to print to. Said out loud
            // rather than swallowed with `let _`, which the workspace lints deny for good reason.
            tracing::debug!("probe receiver is gone");
        }
    });

    let mut count = 0u32;
    while let Ok(action) = rx.recv() {
        count += 1;
        println!("{count:>4}  {action:?}");
    }
}
