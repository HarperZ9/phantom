# License issuance workflow

The end-to-end path from "customer paid" to "customer has a working
license key," plus revocation and investigation. Written for the
licensing operator, not the customer.

Two tools do the work; nothing else is required.

- **`phantom-vendor`** — signs keys, computes serials, verifies
  phone-home payloads. All local — never touches the network.
- **`wrangler`** — Cloudflare CLI. Reads and writes the D1 licenses
  table that the phone-home endpoint queries.

Neither tool has a UI. The workflow is roughly six copy-pastes long.

## Prerequisites

- `phantom-vendor` binary built against the production master seed.
  See `docs/master-seed-rotation.md`. `phantom-vendor` and the
  Cloudflare Worker MUST have been built with the same seed — if
  they diverge, every proof verification will fail and every
  legitimate install will be told it's revoked.
- `wrangler login` completed for the operator's Cloudflare account.
- `endpoints/wrangler.toml` populated with the production
  `account_id`, `database_id`, and KV namespace id (from
  `wrangler.toml.example`).
- Access to the customer intake queue (email inbox, ticket system,
  wherever enrollments land).

## 1. Intake

Customer runs `phantom license request` on their machine. It emits an
enrollment blob:

```json
{
  "schema": 1,
  "fingerprint_hex": "deadbeefcafef00d0011223344556677",
  "requested_tier": "Pro",
  "platform": "windows-x86_64",
  "phantom_version": "0.6.0",
  "master_key_generation": 2
}
```

The customer emails or uploads this. Match it to a paid receipt
(Stripe, invoice, wire — whatever). Confirm the requested tier
matches what they paid for. If `master_key_generation` doesn't
match this vendor's current seed generation, tell the customer to
upgrade before proceeding — an old-generation install cannot
validate keys signed with the new seed.

Record the intake in whatever CRM you use (or a plain spreadsheet):
customer id, email, tier, expiration date, fingerprint.

## 2. Issue the key

```bash
phantom-vendor issue \
  --tier pro \
  --fingerprint deadbeefcafef00d0011223344556677 \
  --expires-days 365
```

Output:

```
License issued.

  key         : AEA6W-UAAAD-GVAAA-A32W3-...
  serial      : 265a67cc
  tier        : Pro
  fingerprint : deadbeefcafef00d0011223344556677
  expires     : epoch-day 20715 (365 days from now)

Record `serial` in D1 so phone-home lookups succeed.
Deliver `key` to the customer via a secure channel.
```

For scripting, add `--json`.

## 3. Record in D1

The endpoint's phone-home lookup fails until the serial exists in
D1. Insert the row:

```bash
wrangler d1 execute phantom-licenses --command "
INSERT INTO licenses (
  serial, customer_id, tier, fingerprint_hex,
  issued_epoch_days, expires_epoch_days,
  issued_at, last_seen_at, revoked_at, note
) VALUES (
  '265a67cc',
  'cust-2026-0417-acme',
  'Pro',
  'deadbeefcafef00d0011223344556677',
  20350, 20715,
  $(date +%s), NULL, NULL, NULL
);"
```

`issued_epoch_days` is today in Unix days (`$(( $(date +%s) / 86400 ))`).
`expires_epoch_days` is what `phantom-vendor issue` printed. The two
must match what was fed to `--fingerprint` and `--expires-days`
exactly — the worker reconstructs the license key from these fields
and the master seed, then verifies the proof against the
reconstructed key. Any drift → proof failure → customer's install is
told it's revoked. Copy-paste, don't retype.

Verify the row landed:

```bash
wrangler d1 execute phantom-licenses --command \
  "SELECT serial, customer_id, tier, expires_epoch_days
   FROM licenses WHERE serial = '265a67cc';"
```

## 4. Deliver

Send the customer the license key over a channel you'd send an
invoice on — encrypted email, the customer portal, a signed message.
The key is bearer material until the fingerprint binds it: whoever
holds this string can activate on the machine that produced the
fingerprint, and nobody else. It is not, however, a permanent
credential — a compromised install can be revoked (§6).

Do **not** paste the key into public issue trackers, support
transcripts a broader team can read, or Slack channels that log
externally.

## 5. Customer activates

The customer runs:

```
phantom license activate AEA6W-UAAAD-GVAAA-A32W3-...
```

Local validation checks the key against the machine's own
fingerprint. If it matches, the tier flips to Pro and the install
starts phoning home every 24h.

The first successful phone-home writes `last_seen_at`. Confirm:

```bash
wrangler d1 execute phantom-licenses --command \
  "SELECT serial, last_seen_at FROM licenses WHERE serial = '265a67cc';"
```

If `last_seen_at` is still NULL after ~24h and the customer says
things are working, check the worker logs (`wrangler tail`) for
`proof_invalid` or `unknown_serial` on that serial — a mismatch
between what was inserted in step 3 and what `phantom-vendor issue`
signed in step 2.

## 6. Revoke

When a license needs to be pulled (charge-back, sharing violation,
customer request, key leaked):

```bash
wrangler d1 execute phantom-licenses --command "
UPDATE licenses SET
  revoked_at = $(date +%s),
  note = 'chargeback 2026-04-18 ticket #4711'
WHERE serial = '265a67cc';"
```

The customer's install picks this up on its next phone-home (worst
case: 24h). The install then downgrades to Free tier silently — the
tripwire path — from the following launch onward. If the customer
disputes, `note` is the audit trail; keep it specific.

To un-revoke (rare — support mistake, appeal upheld):

```bash
wrangler d1 execute phantom-licenses --command \
  "UPDATE licenses SET revoked_at = NULL, note = 'reinstated 2026-04-19'
   WHERE serial = '265a67cc';"
```

## 7. Investigate a phone-home log entry

If a support ticket references a phone-home payload the customer
captured, or the worker log shows a `proof_invalid` event, verify
the payload locally:

```bash
# The customer or worker log gave you a payload file.
# Fetch the current license key by looking up the row and
# regenerating (deterministic from row + seed):
phantom-vendor verify-callback ./payload.json \
  --key AEA6W-UAAAD-GVAAA-A32W3-...
```

Verdicts:

- `OK` — everything checks. The payload is authentic.
- `SERIAL_MISMATCH` — the `license_serial` in the payload does not
  come from the key given. Either the wrong key was passed, or the
  install is running with a different key than expected.
- `PROOF_INVALID` — the payload was not signed by this key.
  Somebody has the serial (from a log) but not the key.
- `TIER_MISMATCH` — the payload's `tier` field disagrees with what
  the key encodes. Tampered install.
- `STALE` — the payload's `unix_secs` is > 15 min from now.
  Either a replay of an old payload, or an install with a wildly
  wrong clock.

`SERIAL_MISMATCH` and `PROOF_INVALID` are the flags that most often
matter: they say "somebody who is not this customer has this
serial." Escalate to the customer — their install may be
compromised.

## 8. List and audit

Ad-hoc queries against D1 for reporting:

```bash
# Active seat count by tier:
wrangler d1 execute phantom-licenses --command "
  SELECT tier, COUNT(*) FROM licenses
  WHERE revoked_at IS NULL AND
        (expires_epoch_days = 0 OR
         expires_epoch_days >= $(( $(date +%s) / 86400 )))
  GROUP BY tier;"

# Licenses that haven't phoned home in 30 days (churn candidates):
wrangler d1 execute phantom-licenses --command "
  SELECT serial, customer_id, tier, last_seen_at
  FROM licenses
  WHERE revoked_at IS NULL
    AND (last_seen_at IS NULL OR last_seen_at < $(( $(date +%s) - 30*86400 )))
  ORDER BY last_seen_at NULLS FIRST;"

# Recently revoked:
wrangler d1 execute phantom-licenses --command "
  SELECT serial, customer_id, revoked_at, note
  FROM licenses
  WHERE revoked_at IS NOT NULL
  ORDER BY revoked_at DESC LIMIT 20;"
```

## What NOT to do

- **Do not** commit `wrangler.toml` — it carries account and
  database ids. `.gitignore` blocks it; a live copy is per-operator.
- **Do not** paste license keys or the master seed into tickets,
  chat, or public dashboards. Keys are bearer material for the
  bound machine; the seed forges every future key.
- **Do not** hand-edit `fingerprint_hex` in D1 after issuance — the
  reconstructed key changes and every phone-home returns
  `revoked: true`. Re-issue instead.
- **Do not** re-use a serial you previously revoked for a new
  customer. Issue a fresh key; the serial is derived from the key,
  so a fresh key gets a fresh serial automatically. Overwriting a
  revoked row's `revoked_at` back to NULL to "reactivate" for a
  different customer is a bug — the fingerprint no longer matches
  and everything the new customer does fails.
- **Do not** run `phantom-vendor issue` against a debug-build of
  the crate. Debug builds use the placeholder seed; keys they emit
  will not validate under production. `phantom-vendor --version`
  prints the master key generation; confirm it matches production
  before issuing.
