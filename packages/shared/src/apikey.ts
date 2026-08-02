/**
 * A readability check for a pasted SteamGridDB API key.
 *
 * This exists to catch the paste accident, offline and instantly, before the user waits on a
 * round trip that will only tell them "rejected": a truncated copy, the word `Bearer` on its own,
 * a whole label dragged along with the value.
 *
 * **It is a hint, never a gate.** `ApiKey::new` in `crates/griddle-core/src/sgdb/key.rs`
 * deliberately does *not* enforce the 32-hex shape — it notes it at debug level and accepts the
 * key anyway, because that shape is today's observed format rather than a documented guarantee,
 * and refusing a future one would brick the app. A frontend that blocked Save would impose
 * exactly the rule Rust declined to impose. So callers must keep Save enabled and treat a `false`
 * here as something to say, not something to act on.
 *
 * The two implementations are independent on purpose and cannot drift into a correctness bug:
 * the Rust side decides what is *accepted*, this side decides only what is *remarked upon*.
 */

/** Length of the keys SteamGridDB issues today. Mirrors `OBSERVED_KEY_LEN`. */
const OBSERVED_KEY_LEN = 32;

/**
 * Strip a leading `Bearer`, the way `ApiKey::new` does.
 *
 * Split on the first run of whitespace rather than matching a literal `"Bearer "`: the value is
 * trimmed first, so a prefix match would fail on the very input people actually paste. Matching
 * is case-insensitive, and `Bearer` alone leaves an empty string — which is what the Rust side
 * reports as `KeyError::Empty`.
 */
export function stripBearer(raw: string): string {
  const trimmed = raw.trim();
  const match = /^bearer(\s+|$)/i.exec(trimmed);
  return match ? trimmed.slice(match[0].length).trim() : trimmed;
}

/**
 * Whether a pasted value looks like a key SteamGridDB would issue.
 *
 * `false` for an empty string, so a caller can ask this without first checking for one — but note
 * that an empty box is not a mistake worth remarking on, and the welcome screen suppresses the
 * hint until something has been typed.
 */
export function looksLikeApiKey(raw: string): boolean {
  const key = stripBearer(raw);
  return key.length === OBSERVED_KEY_LEN && /^[0-9a-f]+$/i.test(key);
}
