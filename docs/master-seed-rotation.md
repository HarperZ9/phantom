# Master signing seed — rotation runbook

The 32-byte master seed baked into every Phantom release is the ONE
value that authenticates a license key. If it leaks, every license
becomes forgeable and the vendor's licensing revenue depends on
rotating the seed and every customer's key with it.

This document covers three things:

1. How the seed is sourced at build time
2. How to generate + install a new seed for the first vendor release
3. How to rotate the seed if it leaks, and what breaks

## How the seed is sourced

`phantom-license/build.rs` resolves the seed by precedence, highest
first:

| Priority | Source | When it's used |
|---|---|---|
| 1 | `PHANTOM_MASTER_SEED` env var | CI release builds. Populated from a repo/org secret. |
| 2 | `.master_seed` file at workspace root | Local vendor builds. Git-ignored. |
| 3 | Compiled DEV placeholder | `cargo build` (debug) only. **`cargo build --release` refuses.** |

If a release build finds no seed via 1 or 2, `build.rs` panics with
a clear error rather than silently signing licenses with the public
placeholder. Verified end-to-end: `cargo build --release` without
either source aborts.

The plaintext seed is XOR-scrambled into `OBFUSCATED_MASTER` and
only the scrambled bytes ship — the seed itself is never in the
compiled binary regardless of source.

## Bake the first real vendor seed

Do this once, before cutting the first customer-facing release.

```bash
# Generate a fresh 32-byte hex seed. openssl on any modern OS.
openssl rand -hex 32
# → 8f4c...bc93   (64 hex chars, 32 bytes)
```

**In GitHub Actions**:

1. Repo → Settings → Secrets and variables → Actions → New
   repository secret
2. Name: `PHANTOM_MASTER_SEED`
3. Value: the 64-hex string, no whitespace
4. Save

The release workflow already reads it via
`env: PHANTOM_MASTER_SEED: ${{ secrets.PHANTOM_MASTER_SEED }}`.

**For local vendor builds** (e.g. to test license issuance against
a real endpoint):

```bash
echo '8f4c...bc93' > .master_seed
# .master_seed is git-ignored, but double-check before committing:
git status --porcelain | grep -q '^\?\? \.master_seed$' && echo "OK — ignored"
```

Confirm: on the next `cargo build --release -p phantom-license`,
the generated `master_key_obf.rs` comment reads
`Seed source: env:PHANTOM_MASTER_SEED` (or `file:../.master_seed`).
`phantom self-check --json` reports `"master_key_generation": 2`.

## What bumping the seed breaks

Every license key ever issued against the previous seed becomes
invalid. Concretely:

- **Every customer install** must re-activate with a newly-issued
  key. Their `phantom license status` will report Free tier on the
  first launch after upgrading past the seed bump.
- **Every phone-home log entry** on the vendor side becomes
  unverifiable — the license serials referenced there were derived
  under the old seed.
- **Every origin_mark on every profile a customer already generated**
  loses its `phantom-verify` verifiability. Profiles still load and
  work; they just show `Unmarked`-equivalent to the new verifier.

What doesn't break:

- Customer profiles keep loading. The origin_mark check is a
  *policy*, not a *decrypt* — verification failure at import shows
  as `Unmarked` in the CLI, and the profile applies fine on the
  local machine.
- `PhantomConfig` and `.license.json` state files re-sign
  themselves on the next save; users don't need to reset anything
  other than their license key.
- CLI settings, ToU/Privacy acknowledgments, profile files — all
  untouched.

**This is not a graceful upgrade.** It is the "we got compromised,
we're re-issuing" flow. Do not perform for cosmetic reasons.

## Actually rotating

Once real customers hold keys, seed rotation is a coordinated
event. Order of operations:

1. **Freeze new license issuance.** Turn off the admin `issue`
   CLI until step 6.
2. **Generate the new seed** with `openssl rand -hex 32`. Store it
   somewhere durable (a password manager, an HSM) BEFORE putting it
   in CI.
3. **Update the GitHub secret** `PHANTOM_MASTER_SEED` to the new
   value.
4. **Cut a new patch release** (e.g. `v1.1.0` → `v1.1.1`) that ships
   binaries built under the new seed. CI verifies release build
   succeeds; no source change needed beyond the version bump. The
   generation number in the generated file goes to 3 (or the next
   integer above the current highest).
5. **Notify customers.** Email or in-app: "we've rotated our
   signing infrastructure; your existing license will need a
   one-time reissue. Follow this link, we'll email you a new key
   within 24h."
6. **Bulk-issue replacement keys** using the vendor admin CLI
   against the new seed, one per active customer. Mark all old
   serials revoked in D1.
7. **Un-freeze issuance.**

Expect a burst of support load for 1–2 weeks after rotation.

## What DOESN'T rotate the seed

- Bumping the crate version (`0.6.0` → `0.7.0`). Version bumps
  keep the seed; only the seed source changes trigger rotation.
- Master key generation bump alone. The generation number is a
  cosmetic marker for `phantom self-check`; it doesn't change what
  key is signing.
- Rebuilding CI. As long as `PHANTOM_MASTER_SEED` is unchanged,
  the seed is the same and licenses stay valid.

## Non-vendor forks

If someone forks Phantom, builds their own release binaries with
their own `PHANTOM_MASTER_SEED`, and issues their own licenses:

- Their licenses are worthless on official builds (different
  master).
- Official licenses are worthless on their builds (different
  master).
- `phantom-verify` binaries need to match the seed they were built
  with. A fork's verifier can't inspect vendor-issued marks and
  vice versa.
- This is intentional. The Proprietary License forbids
  redistribution regardless, but the seed separation makes
  redistribution useless in practice.

## Auditing which seed you're on

`phantom self-check --json` reports `master_key_generation`:
- `1` — you're running a build against the DEV placeholder seed.
  Do not distribute.
- `2` — you're running a build against a real seed (env or file).
  Production-eligible.
- Higher — subsequent rotations.

An operator seeing generation 1 in a supposedly-official install
should file a security report. A generation 2 install matches the
seed baked in at the release CI at that release's build time.
