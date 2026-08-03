/**
 * First run: the one screen between installing Griddle and using it.
 *
 * **Task first, and the policy not at all.** This screen used to open with why the key is the
 * user's own rather than shipped — a good argument, aimed at someone who had not yet been told
 * what they were being asked for. That moved to the bottom, and has now gone entirely.
 *
 * The closing line was three sentences: where the key is stored, that it never rides along with
 * image downloads, and why shipping a shared key fails. Only the first is something a user acts
 * on. It is now one sentence, and the argument lives in `docs/start/your-api-key/`, which is where
 * somebody who actually wants it will look.
 *
 * **The rule for copy in this app: say what the reader does next, or what just happened to them.
 * Reasoning belongs in the docs.** The exceptions are places where the reasoning changes
 * what a reasonable person would click — the reset confirmation says artwork from other tools goes
 * too, because that changes the decision. "We don't ship a shared key because it would get
 * scraped" changes nothing anyone does.
 *
 * **Not a wizard, deliberately.** There is exactly one thing to do, and Next/Back around a single
 * field is ceremony. The other things a setup wizard might walk through — locating Steam, enabling
 * live apply — already happen silently at startup, and the screen that used to ask about one of
 * them was deleted for being unnecessary. This does not bring it back.
 *
 * The numbered steps mirror `docs/start/your-api-key.mdx`. One set of instructions for one task is
 * the point — two is how they drift.
 *
 * Steps 2 and 3 are identical. Steps 1 and 4 differ, and only in ways the medium forces: this
 * screen can say "Paste it below" because the field is right there, and the docs page cannot, so
 * it names Settings instead. Keep the *substance* in step; do not reword either one for its own
 * sake. (This comment used to claim "word-for-word", which was never true of step 4 — an overclaim
 * that would eventually be discovered as a discrepancy and fixed in the wrong direction.)
 */
import lockup from '../assets/logo.png';
import { ExternalLink, KeyEntry } from '../components';
import type { Status } from '../api';

/** Where the user generates their own key. Allowlisted in `griddle_core::browser`. */
const KEY_PAGE = 'https://www.steamgriddb.com/profile/preferences/api';

export function Welcome({
  status,
  onStatus,
}: {
  status: Status;
  onStatus: (s: Status) => void;
}) {
  // A key is stored but unusable — nearly always settings carried from another Windows account,
  // since DPAPI seals to the user. Worth its own opening, because "welcome, get started" is a
  // strange thing to say to someone who did this months ago on another machine.
  const stale = status.key_unreadable;

  return (
    <section className="welcome">
      {/* This screen renders without the app header -- see `Shell`'s `bare` -- so the wordmark
          here is the only one on the page rather than the second copy of it.

          `alt=""` and a real `h1` below, rather than the reverse. The heading carries the name
          for a screen reader, which leaves this decorative in the precise sense the attribute
          means. Dimensions are explicit so the card does not reflow when the image decodes. */}
      <div className="welcome-head">
        <img className="wordmark" src={lockup} alt="" width={320} height={210} />
        {stale ? (
          <>
            <h1>Enter your API key again</h1>
            <p className="lead">
              Griddle found a saved key but could not read it. Keys are encrypted for one Windows
              account, so a key saved elsewhere will not open here. Nothing else was lost.
            </p>
          </>
        ) : (
          <>
            <h1>Welcome</h1>
            <p className="lead">
              Browse SteamGridDB and apply artwork to your Steam library. It appears straight
              away, with no Steam restart.
            </p>
          </>
        )}

        <p>
          {stale ? 'Paste it below, or generate a new one.' : 'First, a SteamGridDB API key.'} It
          is free, and it takes about a minute.
        </p>
      </div>

      <ol className="steps">
        <li>
          Sign in at <strong>steamgriddb.com</strong> (a Steam login works).
        </li>
        <li>
          Open your <strong>profile → Preferences → API</strong>.
        </li>
        <li>Generate a key and copy it.</li>
        <li>Paste it below.</li>
      </ol>

      <p className="row">
        <ExternalLink href={KEY_PAGE} className="linkbutton">
          Open SteamGridDB
        </ExternalLink>
      </p>

      <KeyEntry onStatus={onStatus} autoFocus />

      <p className="hint">
        Your key is stored encrypted for your Windows account, and only ever sent to SteamGridDB.
      </p>

      {/* Said plainly rather than left to be discovered as an empty library. Not a blocker: the
          key is worth saving either way, and Steam may simply not be installed yet. */}
      {status.steam_error && (
        <p className="note note-bad">
          Steam was not found on this PC, so Griddle cannot list your games yet. You can still
          save your key. ({status.steam_error})
        </p>
      )}
    </section>
  );
}
