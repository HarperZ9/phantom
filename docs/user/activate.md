# Activating your license

Phantom starts in Free tier, which already applies the Layer-2 registry
identity set and keeps two profiles. A Pro or Enterprise key raises the
profile limit, enables the background service, and unlocks the deferred
layers as they ship. See [licensing.md](licensing.md) for the tiers.

## Prerequisites

- Phantom installed (see [install.md](install.md)).
- A license key: a long, dash-separated string like
  `AEA6Y-UAAAD-HFAAA-...-QOZ7R-K`.
- An Administrator terminal on the machine the key was issued for.

## What "bound to a machine" means

Every key is bound to one machine at issue time. Your machine
fingerprint, a hash of stable hardware identifiers, was part of what you
sent during purchase (via `phantom license request`). The key only
validates on that same fingerprint. Moving the key to a different
machine will not work; ask your contact for a re-issue.

If your motherboard, primary NIC, or CPU change after issue, the
fingerprint may drift and the key stops validating. Send your license
serial (`phantom license status`) to your contact and they will re-issue
against the new fingerprint.

## 1. Activate the key

```
phantom license activate AEA6Y-UAAAD-HFAAA-...-QOZ7R-K
```

Phantom shows the two documents you agree to before it does anything:

- **Terms of Use**: what you promise not to do with the tool.
- **Privacy Notice**: what phones home, when, and why.

Answer `y` at each prompt. The Terms prompt defaults to No, so an empty
answer declines and activation aborts. For unattended installs, pass
`--accept-tou --acknowledge-privacy-notice` instead of answering.

## 2. Confirm it worked

```
phantom license status
```

Expected:

```
Tier             : Pro
License serial   : 265a67cc
Expires          : 2027-04-17 (365 days remaining)
```

If you see `Tier : Free` after activating, `phantom license status`
explains why: proof failure, fingerprint mismatch, or expired key.

## Rate limits

Phantom rate-limits activation attempts: five per hour, with
exponential back-off after failures (30 seconds, doubling up to an
hour). This is anti-brute-force, not a purchasing limit. If you mistyped
a key and hit the limit, wait; if you lost your key, contact your
licensing contact.

## Phone-home is off until you enable it

Activation records that you acknowledged the Privacy Notice, but no
license check leaves the machine until you set a callback URL. See
[privacy.md](privacy.md) to enable, configure, or keep it off.

## Next steps

- [Create your first profile](first-profile.md).
- Run `phantom privacy-notice` any time to reread what phones home.
