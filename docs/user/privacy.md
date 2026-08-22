# Privacy and phone-home

Phantom is a privacy tool, so it holds itself to the same standard it
gives you: it sends as little as possible, only when you ask it to, and
it shows you exactly what left the machine.

Run `phantom privacy-notice` at any time to read the notice you accept
at activation.

## Off unless you turn it on

Phantom does not phone home by default. There is no vendor URL baked
into the build. Until you set a callback URL, no license check ever
leaves the machine:

```
phantom config set phone_home_url https://your-endpoint/license/callback
```

Setting the URL is what enables phone-home; clearing it turns phone-home
off entirely:

```
phantom config set phone_home_url none
```

## What a call contains

When phone-home is on, each call sends a small, signed payload:

- an **opaque license serial** (a one-way hash of your key; the key
  cannot be recovered from it),
- the **tier** the install believes it holds,
- the **Phantom version**,
- the **wall-clock second** of the call,
- **tripwire counts** (how many local tamper events are on file).

It does **not** send your hardware fingerprint, profile names or
content, seeds, IP-linkable identifiers, or any operator identity. The
payload carries a proof-of-possession signature so the endpoint can tell
a real licensed install from someone who scraped a serial, without the
serial ever revealing the key.

## How often

At most once per interval, default 24 hours. The last-call time is
recorded in a signed file, so restarting a process cannot burst the
endpoint. The transport is `curl`, so the call is visible to any host
monitoring you run, and you can route it through a proxy the normal way.

## Network failures never degrade the tool

A timeout, a DNS failure, an offline laptop: none of these change how
Phantom behaves. The license logic proceeds as if the call succeeded.
The only response Phantom acts on is an explicit revocation.

## Turning it off, and back on

```
phantom config set phone_home_enabled false   # opt out, keep the URL
phantom config set phone_home_enabled true     # opt back in
```

With phone-home off, your license keeps working locally until it
expires and cannot be revoked remotely.

## The local call log

Every attempt is logged locally and HMAC-signed, so you can audit
exactly what left the machine and when. The log records a hash of the
callback URL rather than the URL itself, so reading it does not reveal
every endpoint you have used.
