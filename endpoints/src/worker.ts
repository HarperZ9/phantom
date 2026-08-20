// Phantom license phone-home Worker.
//
// One public route: POST /license/callback. Every phantom install
// hits it once every 24h with a signed payload. The Worker:
//   1. Rate-limits per serial (KV).
//   2. Looks up the serial in D1.
//   3. Reconstructs the license key from the D1 row + master seed.
//   4. Verifies the payload's proof-of-possession.
//   5. Updates last_seen_at.
//   6. Returns { revoked: bool }.
//
// A malformed / unknown / forged payload returns { revoked: true }
// — the install downgrades to Free on the next load. This is the
// intentional posture: strict server-side, fail-open on the client
// only for network errors (not for explicit revocation).

import {
    hexToBytes,
    licenseSerialFor,
    PhoneHomePayload,
    reconstructLicenseKey,
    verifyPhoneHomeProof,
} from "./crypto";

export interface Env {
    DB: D1Database;
    RATE_LIMITS: KVNamespace;
    PHANTOM_MASTER_SEED: string; // 64-hex secret
}

interface LicenseRow {
    serial: string;
    customer_id: string;
    tier: string;
    fingerprint_hex: string;
    issued_epoch_days: number;
    expires_epoch_days: number;
    issued_at: number;
    last_seen_at: number | null;
    revoked_at: number | null;
    note: string | null;
}

const RATE_LIMIT_WINDOW_SECS = 60 * 60; // 1 hour
const RATE_LIMIT_MAX_CALLS = 20; // per serial per window
const RESP_HEADERS = {
    "content-type": "application/json",
    "cache-control": "no-store",
};

export default {
    async fetch(req: Request, env: Env): Promise<Response> {
        const url = new URL(req.url);

        if (req.method === "POST" && url.pathname === "/license/callback") {
            return handleCallback(req, env);
        }
        if (req.method === "GET" && url.pathname === "/health") {
            return json({ ok: true, service: "phantom-licenses" });
        }
        return json({ ok: false, error: "not found" }, 404);
    },
};

async function handleCallback(req: Request, env: Env): Promise<Response> {
    // Parse — malformed payload → revoked. Attackers get no signal.
    let payload: PhoneHomePayload;
    try {
        payload = (await req.json()) as PhoneHomePayload;
    } catch {
        return json({ ok: false, revoked: true, reason: "malformed" }, 400);
    }
    if (
        typeof payload.license_serial !== "string" ||
        typeof payload.tier !== "string" ||
        typeof payload.proof !== "string" ||
        typeof payload.unix_secs !== "number"
    ) {
        return json({ ok: false, revoked: true, reason: "malformed" }, 400);
    }

    // Rate limit per serial. An attacker who scrapes serials off
    // logs and floods the endpoint gets 429'd; a legitimate
    // install phones once per 24h so this ceiling is generous.
    const rlKey = `rl:${payload.license_serial}`;
    const rlCount = parseInt((await env.RATE_LIMITS.get(rlKey)) ?? "0", 10);
    if (rlCount >= RATE_LIMIT_MAX_CALLS) {
        return json({ ok: false, error: "rate_limited" }, 429);
    }
    await env.RATE_LIMITS.put(rlKey, String(rlCount + 1), {
        expirationTtl: RATE_LIMIT_WINDOW_SECS,
    });

    // Lookup the license row. Unknown serial → revoked (either
    // never issued or already deleted).
    const row = await env.DB.prepare(
        "SELECT serial, customer_id, tier, fingerprint_hex, issued_epoch_days, expires_epoch_days, issued_at, last_seen_at, revoked_at, note FROM licenses WHERE serial = ?"
    )
        .bind(payload.license_serial)
        .first<LicenseRow>();

    if (!row) {
        return json({ ok: false, revoked: true, reason: "unknown_serial" });
    }

    // Already revoked in D1? Instant deny.
    if (row.revoked_at !== null) {
        return json({
            ok: false,
            revoked: true,
            reason: "revoked",
            revoked_at: row.revoked_at,
        });
    }

    // Reconstruct the license key from the D1 row plus the master
    // seed, then verify the payload's proof-of-possession. If the
    // caller doesn't actually hold the key material, the proof
    // fails and we treat it as revoked (they scraped the serial
    // from a log entry, not from a real install).
    let masterSeed: Uint8Array;
    try {
        masterSeed = hexToBytes(env.PHANTOM_MASTER_SEED);
    } catch {
        // Ops error: seed misconfigured. Fail closed to the caller
        // (don't accidentally validate everything) but log so an
        // operator notices.
        console.error("PHANTOM_MASTER_SEED misconfigured");
        return json({ ok: false, revoked: true, reason: "server_config" }, 500);
    }
    if (masterSeed.length !== 32) {
        console.error(`master seed wrong length: ${masterSeed.length}`);
        return json({ ok: false, revoked: true, reason: "server_config" }, 500);
    }

    const tier = row.tier as "Free" | "Pro" | "Enterprise";
    if (tier !== "Free" && tier !== "Pro" && tier !== "Enterprise") {
        console.error(`bad tier in D1: ${row.tier}`);
        return json({ ok: false, revoked: true, reason: "server_config" }, 500);
    }

    const reconstructedKey = await reconstructLicenseKey(
        masterSeed,
        tier,
        row.fingerprint_hex,
        row.issued_epoch_days,
        row.expires_epoch_days
    );

    // Sanity check: serial derived from reconstructed key must
    // equal the serial that arrived on the wire. Catches D1 row
    // corruption or a fingerprint mismatch between what was
    // enrolled and what the row records.
    const expectedSerial = await licenseSerialFor(masterSeed, reconstructedKey);
    if (expectedSerial !== payload.license_serial) {
        console.error(
            `serial reconstruction mismatch: expected=${expectedSerial} got=${payload.license_serial}`
        );
        return json({ ok: false, revoked: true, reason: "server_config" }, 500);
    }

    const proofOk = await verifyPhoneHomeProof(reconstructedKey, payload);
    if (!proofOk) {
        // Attacker knows the serial but not the key material. Log
        // it — repeated proof failures on the same serial are a
        // real signal that a serial has leaked and the actual
        // install may be compromised.
        console.warn(
            `proof failure for serial=${payload.license_serial} ip=${req.headers.get(
                "cf-connecting-ip"
            )}`
        );
        return json({ ok: false, revoked: true, reason: "proof_invalid" });
    }

    // Freshness: reject payloads whose unix_secs is >15 min from
    // now. Blunt anti-replay.
    const now = Math.floor(Date.now() / 1000);
    if (Math.abs(now - payload.unix_secs) > 15 * 60) {
        return json({ ok: false, revoked: true, reason: "stale" });
    }

    // Expiration: if the license expired, tell the install to
    // stop treating itself as licensed.
    if (
        row.expires_epoch_days !== 0 &&
        row.expires_epoch_days < Math.floor(now / 86400)
    ) {
        return json({
            ok: false,
            revoked: true,
            reason: "expired",
            expires_epoch_days: row.expires_epoch_days,
        });
    }

    // All checks pass — record last-seen and return ok.
    await env.DB.prepare(
        "UPDATE licenses SET last_seen_at = ? WHERE serial = ?"
    )
        .bind(now, payload.license_serial)
        .run();

    return json({
        ok: true,
        revoked: false,
        tier: row.tier,
        expires_epoch_days: row.expires_epoch_days,
    });
}

function json(body: unknown, status = 200): Response {
    return new Response(JSON.stringify(body), {
        status,
        headers: RESP_HEADERS,
    });
}
