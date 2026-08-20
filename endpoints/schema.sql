-- Phantom licenses D1 schema.
--
-- One row per issued license. `serial` is the primary key because
-- that's what the phone-home payload carries; every lookup happens
-- by serial.
--
-- License keys themselves are NOT stored — the worker reconstructs
-- them at verify time from (tier, fingerprint_hex, issued_epoch_days,
-- expires_epoch_days) plus the master seed. If this database is
-- dumped, an attacker gets customer records but cannot forge keys.
CREATE TABLE IF NOT EXISTS licenses (
    serial              TEXT PRIMARY KEY,
    -- Which customer this key was issued to. Free-form; typically
    -- an internal customer id or an email address for support.
    customer_id         TEXT NOT NULL,
    -- 'Free' | 'Pro' | 'Enterprise'.
    tier                TEXT NOT NULL,
    -- 32-hex machine fingerprint the customer sent at enrollment.
    fingerprint_hex     TEXT NOT NULL,
    -- Unix days from epoch.
    issued_epoch_days   INTEGER NOT NULL,
    -- 0 = perpetual.
    expires_epoch_days  INTEGER NOT NULL,
    -- Unix seconds when the row was written (server side).
    issued_at           INTEGER NOT NULL,
    -- Unix seconds of the most recent successful phone-home.
    last_seen_at        INTEGER,
    -- Unix seconds when the license was revoked. NULL = active.
    revoked_at          INTEGER,
    -- Free-form ops note (why revoked, appeal state, etc.).
    note                TEXT
);

CREATE INDEX IF NOT EXISTS licenses_customer ON licenses(customer_id);
CREATE INDEX IF NOT EXISTS licenses_revoked ON licenses(revoked_at)
    WHERE revoked_at IS NOT NULL;
