# Licensing

Phantom is a licensed product with three tiers. It runs in Free tier
the moment it is installed, with no key required.

## Tiers

| Tier | Layers | Profiles | Background service |
|------|--------|----------|--------------------|
| Free | Layer 2 (registry) | 2 | No |
| Pro | All layers as they ship | 50 | Yes |
| Enterprise | All layers as they ship | Unlimited | Yes |

Free is a real tier, not a trial. It applies the full Layer-2 registry
identity set (see the README's "What apply changes") and keeps up to
two saved profiles. Pro and Enterprise raise the profile limit, enable
the background service that re-applies your profile across reboots, and
unlock Layer 1 and Layer 0 as those ship in later releases.

## Getting a key

A key is issued to one machine. The flow is:

1. On the machine you want licensed, run:

   ```
   phantom license request
   ```

   This prints an enrollment block: your machine fingerprint, the tier
   you want, and build details. It contains no personal data and no
   profile content.

2. Send that block to your Phantom licensing contact.

3. They issue a key bound to your fingerprint and send it back. Activate
   it (see [activate.md](activate.md)).

## Keys are bound to one machine

A key is a long, dash-separated string, for example:

```
AEA6Y-UAAAD-HFAAA-...-QOZ7R-K
```

It is HMAC-signed and bound to the hardware fingerprint from your
enrollment request. The same key is worthless on any other machine, so
a leaked key cannot license a second device.

The fingerprint is a hash of stable hardware identifiers. If your
motherboard, primary NIC, or CPU changes, it can drift and the key
stops validating. Run `phantom license status` for the serial and ask
your contact for a re-issue against the new fingerprint.

## Checking status

```
phantom license status
```

Shows the active tier, license serial, and expiry. If a key you
activated is not honored, the status explains why: proof failure,
fingerprint mismatch, or expiry.

## Revocation

Revocation depends on phone-home, which is off until you configure a
callback URL (see [privacy.md](privacy.md)). If an operator has enabled
phone-home and the licensing side revokes a serial, the next check
returns revoked and the install drops to Free on its following run:
Layer 2 keeps working, and the Pro-only layers and higher profile limit
are withdrawn. With phone-home off, a key keeps working locally until it
expires and cannot be revoked remotely. That tradeoff is yours to make.

## Deactivating

```
phantom license deactivate
```

Returns the install to Free tier and clears the stored key. Your
profiles are kept.
