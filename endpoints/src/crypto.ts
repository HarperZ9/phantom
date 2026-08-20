// Crypto primitives the license flow needs, implemented on the
// Web Crypto API (available in Cloudflare Workers, Deno, browsers).
// Every constant and byte layout here MUST match phantom-license
// exactly — the client crate is the source of truth and this file
// is the mirror.

// Domain-separation strings — must match phantom-license/src/keys.rs.
const LICENSE_PURPOSE = new TextEncoder().encode("phantom.license.v1");
const STATE_PURPOSE = new TextEncoder().encode("phantom.state.v1");

// From phantom-license/src/phone_home.rs — the exact bytes fed
// into the serial HMAC.
const SERIAL_DOMAIN = new TextEncoder().encode("phantom.license-serial.v1");

// From phantom-license/src/phone_home.rs — proof-of-possession
// domain-separation.
const PROOF_DOMAIN = new TextEncoder().encode("phantom.phone-home-proof.v1");

/**
 * Decode a hex string of even length into a Uint8Array.
 * Throws if the input isn't valid hex.
 */
export function hexToBytes(hex: string): Uint8Array {
    const clean = hex.trim().toLowerCase();
    if (clean.length % 2 !== 0) {
        throw new Error(`hex length ${clean.length} is not even`);
    }
    const out = new Uint8Array(clean.length / 2);
    for (let i = 0; i < out.length; i++) {
        const byte = parseInt(clean.slice(i * 2, i * 2 + 2), 16);
        if (Number.isNaN(byte)) {
            throw new Error(`invalid hex at byte ${i}`);
        }
        out[i] = byte;
    }
    return out;
}

export function bytesToHex(bytes: Uint8Array): string {
    let s = "";
    for (const b of bytes) s += b.toString(16).padStart(2, "0");
    return s;
}

/** Constant-time byte-array equality. */
export function ctEq(a: Uint8Array, b: Uint8Array): boolean {
    if (a.length !== b.length) return false;
    let diff = 0;
    for (let i = 0; i < a.length; i++) diff |= a[i] ^ b[i];
    return diff === 0;
}

/** HMAC-SHA256(key, ...msgParts) → 32-byte Uint8Array. */
export async function hmacSha256(
    key: Uint8Array,
    ...msgParts: Uint8Array[]
): Promise<Uint8Array> {
    const cryptoKey = await crypto.subtle.importKey(
        "raw",
        key.buffer.slice(key.byteOffset, key.byteOffset + key.byteLength),
        { name: "HMAC", hash: "SHA-256" },
        false,
        ["sign"]
    );
    // Concatenate parts — Web Crypto sign() takes one buffer.
    let total = 0;
    for (const p of msgParts) total += p.length;
    const buf = new Uint8Array(total);
    let off = 0;
    for (const p of msgParts) {
        buf.set(p, off);
        off += p.length;
    }
    const sig = await crypto.subtle.sign("HMAC", cryptoKey, buf);
    return new Uint8Array(sig);
}

/**
 * Domain-separated key derivation. Matches
 * `phantom_license::keys::derive_key(purpose)`.
 * Returns a 32-byte subkey.
 */
export async function deriveKey(
    masterSeed: Uint8Array,
    purpose: Uint8Array
): Promise<Uint8Array> {
    return await hmacSha256(masterSeed, purpose);
}

/**
 * Compute the opaque 8-hex license serial for a key. Matches
 * `phantom_license::phone_home::license_serial_for(Some(key))`.
 */
export async function licenseSerialFor(
    masterSeed: Uint8Array,
    licenseKey: string
): Promise<string> {
    const stateKey = await deriveKey(masterSeed, STATE_PURPOSE);
    const mac = await hmacSha256(
        stateKey,
        SERIAL_DOMAIN,
        new TextEncoder().encode(licenseKey)
    );
    return bytesToHex(mac.slice(0, 4));
}

/**
 * Verify the phone-home proof-of-possession. Matches
 * `phantom_license::phone_home::verify_proof(key, payload)`.
 * The payload's `proof` field is compared against
 * HMAC-SHA256(license_key_bytes, PROOF_DOMAIN || canonical_payload).
 * Canonical bytes = JSON serialization with proof cleared.
 */
export async function verifyPhoneHomeProof(
    licenseKey: string,
    payload: PhoneHomePayload
): Promise<boolean> {
    if (!payload.proof) return false;

    // Canonical bytes: serialize with proof cleared. We must
    // match serde_json's field order, which for a struct is
    // declaration order: schema, license_serial, tier,
    // phantom_version, unix_secs, trip_count_low, trip_count_high,
    // proof.
    const canonical: PhoneHomePayload = { ...payload, proof: "" };
    const canonicalBytes = new TextEncoder().encode(
        JSON.stringify(canonical)
    );

    const keyBytes = new TextEncoder().encode(licenseKey);
    const expected = await hmacSha256(keyBytes, PROOF_DOMAIN, canonicalBytes);
    const expectedHex = bytesToHex(expected);
    const providedHex = payload.proof.toLowerCase();
    // Constant-time compare on hex strings (same length).
    return ctEq(
        new TextEncoder().encode(expectedHex),
        new TextEncoder().encode(providedHex)
    );
}

/**
 * License key reconstruction. Matches
 * `phantom_license::key::generate_license_key(tier, fingerprint,
 * expires_epoch_days, issued_epoch_days)`.
 *
 * Byte layout (from phantom-license/src/key.rs):
 *   [0]      version (1)
 *   [1]      tier byte (Free=0, Pro=1, Enterprise=2)
 *   [2..6]   expires_epoch_days (LE u32, 0=perpetual)
 *   [6..10]  issued_epoch_days (LE u32)
 *   [10..26] machine hash (16 bytes)
 *   [26..28] reserved (zeros)
 *   [28..60] HMAC-SHA256 signature over bytes [0..28] using
 *            HMAC(deriveKey(master, LICENSE_PURPOSE))
 *
 * Then base32-encoded and dash-separated for display. Used at
 * verify time on the server to reconstruct the exact key the
 * customer's install holds, so proof verification succeeds.
 */
export async function reconstructLicenseKey(
    masterSeed: Uint8Array,
    tier: "Free" | "Pro" | "Enterprise",
    fingerprintHex: string,
    issuedEpochDays: number,
    expiresEpochDays: number
): Promise<string> {
    const fp = hexToBytes(fingerprintHex);
    if (fp.length !== 16) throw new Error(`fingerprint must be 16 bytes, got ${fp.length}`);

    const raw = new Uint8Array(60);
    raw[0] = 1; // KEY_VERSION
    raw[1] = tier === "Free" ? 0 : tier === "Pro" ? 1 : 2;
    // LE u32 expires
    raw[2] = expiresEpochDays & 0xff;
    raw[3] = (expiresEpochDays >>> 8) & 0xff;
    raw[4] = (expiresEpochDays >>> 16) & 0xff;
    raw[5] = (expiresEpochDays >>> 24) & 0xff;
    // LE u32 issued
    raw[6] = issuedEpochDays & 0xff;
    raw[7] = (issuedEpochDays >>> 8) & 0xff;
    raw[8] = (issuedEpochDays >>> 16) & 0xff;
    raw[9] = (issuedEpochDays >>> 24) & 0xff;
    raw.set(fp, 10);
    // 26..28 stay zero (reserved).

    const licenseKey = await deriveKey(masterSeed, LICENSE_PURPOSE);
    const sig = await hmacSha256(licenseKey, raw.slice(0, 28));
    raw.set(sig, 28);

    // Base32 encode + dash-format, matching key.rs base32_encode
    // + format_key_display (groups of 5).
    const encoded = base32Encode(raw);
    return groupsOf5DashSeparated(encoded);
}

// RFC 4648 base32 alphabet (matches phantom-license/src/key.rs).
const BASE32_ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

function base32Encode(bytes: Uint8Array): string {
    let bits = 0;
    let value = 0;
    let out = "";
    for (const b of bytes) {
        value = (value << 8) | b;
        bits += 8;
        while (bits >= 5) {
            out += BASE32_ALPHABET[(value >>> (bits - 5)) & 0x1f];
            bits -= 5;
        }
    }
    if (bits > 0) {
        out += BASE32_ALPHABET[(value << (5 - bits)) & 0x1f];
    }
    return out;
}

function groupsOf5DashSeparated(s: string): string {
    const groups: string[] = [];
    for (let i = 0; i < s.length; i += 5) {
        groups.push(s.slice(i, i + 5));
    }
    return groups.join("-");
}

// -------------------- shared types --------------------

export interface PhoneHomePayload {
    schema: number;
    license_serial: string;
    tier: string;
    phantom_version: string;
    unix_secs: number;
    trip_count_low: number;
    trip_count_high: number;
    proof: string;
}
