# Phantom endpoints — license phone-home service

Minimal Cloudflare Worker + D1 that closes the licensing loop for
Phantom v1.

## What it does

**`POST /license/callback`** — public endpoint every phantom install
calls into once per 24 hours. Looks up the license serial in D1,
verifies the proof-of-possession, updates `last_seen_at`, and
returns `{ok, revoked}`. Also enforces a rolling per-serial rate
limit via KV.

That's it. Every other operation (issue, revoke, list) happens
locally against D1 via `wrangler d1 execute` — see
`../docs/issuance-workflow.md`.

## Deploy

```bash
cd endpoints
npm install
cp wrangler.toml.example wrangler.toml
# Fill in wrangler.toml: account_id, D1 database name/id, KV
# namespace id, custom domain.

# One-time D1 setup:
wrangler d1 create phantom-licenses
wrangler d1 execute phantom-licenses --file=schema.sql

# One-time KV setup:
wrangler kv:namespace create RATE_LIMITS

# Set the master seed secret (same value as the client-side
# PHANTOM_MASTER_SEED GitHub secret from Sprint 24):
wrangler secret put PHANTOM_MASTER_SEED

# Deploy:
wrangler deploy
```

## Local dev

```bash
npm run dev
# → http://localhost:8787
# Test:
curl -X POST http://localhost:8787/license/callback \
  -H 'Content-Type: application/json' \
  --data-binary "$(cat sample-payload.json)"
```

## Security model

- The master seed lives ONLY as a Worker Secret and the GitHub CI
  secret. It never appears in D1, in logs, or on the wire.
- D1 stores `serial → { customer_id, tier, fingerprint_hex,
  issued_at, expires_at, revoked_at, last_seen_at }`. Nothing else.
- License keys themselves are NOT stored in D1 — the vendor doesn't
  need to keep them; only the (serial, fingerprint) pair to verify
  a phone-home. If the vendor tools DB is dumped, an attacker gets
  the customer records but cannot forge a key.
- Actually — because verify-proof needs the full license key on
  server side, the endpoint DOES need the key. Alternative:
  re-derive the key from (tier, fingerprint, expires, issued) since
  those fully determine it. That's what `worker.ts` does — it
  reconstructs the key locally at verify time from the D1 row plus
  the master seed. So D1 never holds a key.
