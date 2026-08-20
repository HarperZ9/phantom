# Phantom license phone-home API

Single public endpoint. Everything else is local ops via `wrangler`.

## `POST /license/callback`

Called by every phantom install once per 24 hours (configurable).

### Request

`application/json` body:

```json
{
  "schema": 1,
  "license_serial": "265a67cc",
  "tier": "Pro",
  "phantom_version": "0.6.0",
  "unix_secs": 1787220000,
  "trip_count_low": 0,
  "trip_count_high": 0,
  "proof": "e7a1...5c9d"
}
```

Field semantics — see `phantom-license/src/phone_home.rs` and the
in-app `phantom privacy-notice` for the authoritative source.

- `schema`: bumped when the payload format changes.
- `license_serial`: opaque 8-hex HMAC of the license key. Not
  reversible; identifies the install to the endpoint.
- `tier`: what the install BELIEVES its tier is. The server
  cross-checks against D1 and against the license key's embedded
  tier byte; a mismatch fails proof verification (any field change
  breaks the proof).
- `phantom_version`: informational.
- `unix_secs`: wall-clock at call time. Must be within ±15 minutes
  of server time or the payload is rejected as stale.
- `trip_count_low` / `trip_count_high`: local tripwire counts for
  vendor visibility.
- `proof`: HMAC-SHA256(license_key_bytes,
  `phantom.phone-home-proof.v1` || canonical_payload). Canonical
  payload = the JSON above with `proof` cleared. This is what
  distinguishes an install actually holding the license from an
  attacker who scraped a serial.

### Response

Success:

```json
{ "ok": true, "revoked": false, "tier": "Pro",
  "expires_epoch_days": 20715 }
```

License invalidated:

```json
{ "ok": false, "revoked": true, "reason": "revoked",
  "revoked_at": 1787200000 }
```

Reasons the server returns `revoked: true`:

| reason | Meaning |
|---|---|
| `malformed` | Payload didn't parse or missed required fields |
| `unknown_serial` | Serial not in D1 (never issued, or deleted) |
| `revoked` | Explicitly revoked via vendor tools |
| `proof_invalid` | HMAC over payload doesn't match the license key |
| `stale` | `unix_secs` outside ±15 min of server time |
| `expired` | License's `expires_epoch_days` past |
| `server_config` | Master seed misconfigured on the Worker |

The client treats any `revoked: true` as a high-severity tripwire
event and silently downgrades to Free tier from the next launch
onward (see `phantom-license/src/phone_home.rs`).

### Rate limits

- 20 calls per hour per `license_serial` (KV-backed).
- 429 Too Many Requests when the ceiling is hit.
- A legitimate install phones home once per 24h, so 20/hr is
  generous. Bursts above indicate a serial has been scraped and
  someone is flooding the endpoint. Support should investigate the
  affected customer's install for compromise.

### Security posture

- **Fail-closed on the server** for explicit revocation / expiry /
  proof failure / stale / unknown-serial. The install is told,
  clearly, that it's revoked.
- **Fail-open on the client** for network errors (timeout, DNS,
  offline). A user with a firewalled network keeps working; only
  an authenticated `revoked: true` from the endpoint downgrades.
- **No fingerprint on the wire**. The endpoint receives an opaque
  serial and reconstructs the license key from D1 metadata + the
  master seed to verify the proof. The customer's real fingerprint
  never leaves the customer's machine.
- **No license keys in the database**. D1 stores the constituent
  parts (tier, fingerprint, dates); the worker reconstructs the
  key at verify time. A D1 dump exposes customer records but not
  forgeable material.

## `GET /health`

Liveness. Returns `{ "ok": true, "service": "phantom-licenses" }`.
