# Activating your license

Phantom starts in Free tier — enough to audit your current hardware
identity but not enough to change it. To unlock Pro or Enterprise
you need a license key, delivered to you at purchase.

## Prerequisites

- Phantom installed (see [install.md](install.md)).
- A license key, formatted `AAAAA-BBBBB-CCCCC-...` (12 groups of 5
  characters, dash-separated).
- Administrator terminal on the machine you bought the key for.

## What "bound to a machine" means

Every key is bound to one machine at issue time. Your machine
fingerprint — a 16-byte hash of stable hardware identifiers — was
part of what you sent us during purchase (via `phantom license
request`). The key we sent back only validates on that same
fingerprint. Moving the key to a different machine will not work;
you have to ask for a re-issue.

If your motherboard, primary NIC, or CPU change after issue, the
fingerprint may drift and the key will stop validating. Email
support with your license serial (`phantom license status`) and we
will re-issue against the new fingerprint at no cost.

## 1. Request activation

```
phantom license activate AAAAA-BBBBB-CCCCC-DDDDD-EEEEE-FFFFF-GGGGG-HHHHH-IIIII-JJJJJ-KKKKK-LLLLL
```

Phantom will show you the two documents you must agree to before it
does anything:

- **Terms of Use** — what you promise not to do with the tool.
- **Privacy notice** — what phones home, when, and why.

Type `agree` at each prompt. If you decline either, activation
aborts and Phantom stays in Free tier.

## 2. Check activation succeeded

```
phantom license status
```

Expected output:

```
Tier             : Pro
License serial   : 265a67cc
Activated at     : 2026-04-17 21:15:03 UTC
Expires          : 2027-04-17 (365 days remaining)
Last phone-home  : (never — first check within 24h)
```

If you see `Tier : Free` after activation, `phantom license status`
will explain why: proof failure, fingerprint mismatch, or expired
key. Contact support with the reason.

## Rate limits

Phantom rate-limits activation attempts: five per hour, with
exponential back-off after failures (30 seconds, then doubling to 1
hour). This is an anti-brute-force measure, not a purchasing limit.
If you fat-fingered a key and hit the limit, wait; if you actually
lost your key, contact support.

## Turning phone-home off

Phone-home is on by default (see the privacy notice you accepted
above). To turn it off:

```
phantom config set phone_home_enabled false
```

Your license keeps working locally until it expires. We can no
longer revoke it remotely — this is a tradeoff you may prefer.

## Turning it back on

```
phantom config set phone_home_enabled true
```

## Next steps

- [Create your first profile](first-profile.md).
- Run `phantom privacy-notice` any time to reread what phones home.
