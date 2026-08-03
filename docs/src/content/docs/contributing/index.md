---
title: Contributing
description: Building Griddle from source, the checks that must pass, and how releases are made.
---

## Building

You need [Rust](https://rustup.rs/) (MSVC toolchain) and [bun](https://bun.sh/). No Node.

```powershell
git clone https://github.com/dahui/griddle
cd griddle
bun install

bun run app              # dev, with hot reload
bun run app:release      # release build -> target\release\griddle-app.exe
bun run app:build        # installer
```

:::caution[Use `app:release`, not `cargo build --release`]
`cargo build` does not build the frontend. A bare `cargo build --release -p griddle-app` embeds
whatever was last written to `apps/desktop/dist`, which may be months old, and everything still
looks correct: it compiles, starts, and titles its window. `bun run app:release` builds both, in
order.
:::

## Before you push

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\gate.ps1
```

This is exactly what CI runs. See [Testing](/griddle/internals/testing/).

Enable the pre-commit hook once, which runs the secret scan on staged changes:

```powershell
git config core.hooksPath .githooks
```

## The API key must never be committed

The SteamGridDB key is a per-user secret, and one *will* end up pasted into a terminal or a test
during development. `scripts/check-secrets.sh` runs as a pre-commit hook and in CI, where it also
scans the full history. CI cannot depend on the hook, since a fresh clone has none configured.

For development, put your key in `SGDB_API_KEY` or a gitignored `.env`.

## Documentation

This site is [Astro Starlight](https://starlight.astro.build/), in `docs/`, deliberately outside
the bun workspace so its dependencies never enter the app's install.

```powershell
cd docs
bun install
bun run dev
```

It deploys automatically when anything under `docs/` reaches `main`.

### Screenshots

`scripts\screenshots.ps1` regenerates the documentation images from a release build, so
recapturing is one command rather than a remembered procedure. Run it as part of a release. A stale
screenshot fails silently, and the docs quietly start describing a version nobody has.

There are two modes, because the API key decides which screens exist:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\screenshots.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\screenshots.ps1 -Welcome
```

The default captures `library`, `browse`, `current` and `settings`, and needs a key stored.
`-Welcome` captures the first-run screen and needs the opposite, so remove your key in the app
first and paste it back afterwards.

Each mode checks its precondition and refuses to run otherwise. **The script never reads, writes,
moves or deletes `settings.json`**, and it should stay that way. An earlier harness reached first
run by moving that file aside, and destroyed a DPAPI-sealed API key twice doing it. A sealed key
cannot be recovered from anything on disk.

It clicks at fixed coordinates, so **look at every image before committing it**. A click that
misses lands somewhere harmless and produces a duplicate of the previous capture rather than an
error. Two things to check specifically: `browse` must show an expanded filter panel, and
`settings` must show no Diagnostics rows, since your Steam account id is in there.

## Releases

Pushing a tag is the entire release action:

```powershell
git tag v1.2.3
git push origin v1.2.3
```

CI stamps the version into every manifest, builds the portable zip and the installer, generates
checksums, and publishes a GitHub release. A tag that is not valid semver fails the build rather
than producing a release named after a typo. Tags with a suffix, like `v1.2.3-rc.1`, publish as
pre-releases.

## Licence

Apache-2.0. Contributions are accepted under the same terms. There is no CLA.

Third-party attributions in `THIRD-PARTY-NOTICES.txt` are generated, and CI fails if they are out
of date. Run `scripts\notices.ps1` after changing dependencies.
