/**
 * The paste hint, held to the same tolerances as `ApiKey::new`.
 *
 * The cases that matter are the ones where the two sides must *agree that a key is fine* — a
 * `Bearer` prefix, surrounding whitespace, mixed case hex. Disagreeing there would put a warning
 * under a key that then saves successfully, which teaches the user to ignore the warning.
 *
 * Every literal here is obviously synthetic. Real keys never appear in this repo; see
 * `scripts/check-secrets.sh`.
 */
import { describe, expect, test } from 'bun:test';
import { looksLikeApiKey, stripBearer } from './apikey';

/** 32 hex characters — the shape SteamGridDB issues today. */
const WELL_FORMED = '0123456789abcdef0123456789abcdef';

describe('stripBearer', () => {
  test('removes a Bearer prefix in any case, with any spacing', () => {
    expect(stripBearer(`Bearer ${WELL_FORMED}`)).toBe(WELL_FORMED);
    expect(stripBearer(`bearer ${WELL_FORMED}`)).toBe(WELL_FORMED);
    expect(stripBearer(`BEARER    ${WELL_FORMED}`)).toBe(WELL_FORMED);
  });

  test('trims surrounding whitespace, including a pasted line break', () => {
    expect(stripBearer(`  ${WELL_FORMED}\r\n`)).toBe(WELL_FORMED);
  });

  test('leaves nothing behind when Bearer is all there is', () => {
    // Rust reports this as `KeyError::Empty` rather than treating "Bearer" as a six-character
    // key, which is what an over-eager `strip_prefix` would do.
    expect(stripBearer('Bearer')).toBe('');
    expect(stripBearer('Bearer ')).toBe('');
  });

  test('does not eat a key that merely starts with the letters', () => {
    // No whitespace boundary, so this is a value, not a label.
    expect(stripBearer('bearerish')).toBe('bearerish');
  });
});

describe('looksLikeApiKey', () => {
  test('accepts the observed shape, in either case', () => {
    expect(looksLikeApiKey(WELL_FORMED)).toBe(true);
    expect(looksLikeApiKey(WELL_FORMED.toUpperCase())).toBe(true);
  });

  test('accepts what ApiKey::new accepts after its own tidying', () => {
    // The hint must not fire on input Rust will happily take, or it contradicts the Save button
    // sitting next to it.
    expect(looksLikeApiKey(`Bearer ${WELL_FORMED}`)).toBe(true);
    expect(looksLikeApiKey(`  ${WELL_FORMED}  `)).toBe(true);
  });

  test('catches the real paste accidents', () => {
    expect(looksLikeApiKey('')).toBe(false);
    expect(looksLikeApiKey('Bearer')).toBe(false);
    expect(looksLikeApiKey(WELL_FORMED.slice(0, 20))).toBe(false); // truncated copy
    expect(looksLikeApiKey(`${WELL_FORMED}0`)).toBe(false); // one too many
    expect(looksLikeApiKey('API key: 0123456789abcdef')).toBe(false); // label came along
  });

  test('rejects 32 characters that are not hex', () => {
    // Length alone is not the predicate: a 32-character sentence would otherwise pass.
    expect('z'.repeat(32)).toHaveLength(32);
    expect(looksLikeApiKey('z'.repeat(32))).toBe(false);
  });
});
