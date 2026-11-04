# Full-stack identity: extending Phantom up and down the line

Status: design. Nothing in this document is built. Date: 2026-11-04.

The author, 2026-11-04: "We should extend Phantom to work on hardware, and physical/digital spaces; all the way up and down the line." The frame set earlier the same day: "we will setup the cleanroom workstation first; with completely limited outbound identifiers, and metadata. then we will build a custom networking protocol for this station, so that it can operate in this way in any network configuration. we want to limit the amount of data that is distilled by the inference boundary of claude and chatgpt."

Inputs read: `README.md`, `CHANGELOG.md`, `docs/kernel-driver-review.md`, the crate layout, `phantom-cli/src/profile/schema.rs`, `phantom-cli/src/apply/mod.rs`, `phantom-license/src/fingerprint.rs` and `guard.rs`, the driver sources, the unmerged branch `codex/phantom-boundary-gateway-20260909`, and the workstation privacy plan at `C:/dev/mission-control/2026-11-04-workstation-efficiency/PRIVACY-PLAN.md` (it exists; section 8 merges it).

## 1. Contract at every layer

Every surface uses the five verbs Phantom already has. A surface that cannot honour a verb says so in `audit` output.

| Verb | Meaning on any surface |
|---|---|
| `audit` | Read the live value, who can read it, and where the value came from. Read-only. Values are masked in output unless `--reveal` is passed. |
| `profile generate` | Derive the surface's values from the profile seed so they agree with each other (a NIC MAC, its DHCP client ID and its DHCPv6 DUID move together). |
| `apply` | Write, after the backup for that surface is on disk and fsynced. A write without a backup entry is refused. |
| `validate` | Re-read every source for the surface and compare to the profile. Reports `consistent`, `mismatch` or `not-available`. |
| `revert` | Restore from the backup exactly. The backup is removed only when every entry reverted. |

Each identifier gets one capability class, shown by `audit`:

- **L2-change**: userland write, reversible, no driver.
- **L1-driver**: needs the kernel filter (`phantom-driver`).
- **L0-firmware**: needs the DXE module (`phantom-dxe`) or device firmware.
- **audit-only**: Phantom reports it and never writes it.
- **guidance**: no software lever; Phantom prints the physical or account step.

**Provenance in audit.** A live value that differs from its hardware or install-time source gets an origin line. The privacy plan found Ethernet 3 running on a locally administered MAC set through `NetworkAddress`, origin unknown. For a case like that, `audit` reports: the permanent value (`OID_802_3_PERMANENT_ADDRESS` or the SMBIOS or IDENTIFY source), the live value, the mechanism holding the override (`NetworkAddress` value, Phantom backup entry, Phantom driver, NIC firmware, or unknown), the registry key's last-write time, and whether a Phantom backup claims it. An override no Phantom backup claims is labelled `foreign override`, and `apply` refuses to write over it until the operator adopts it (`phantom adopt <surface>`), which records the current value as the original to restore. Without that step a revert would restore Phantom's view of the original and silently drop someone else's change.

CLI shape: the numeric `--layers 0,1,2` stays for the identity stack. New surfaces take a `--surfaces` flag: `rf`, `net`, `app`, `telemetry`, `content`, `infer`. `phantom audit --surfaces all` is the station-wide census.

### 1.1 Boundaries, kept per surface

Phantom's README limits it to machines the operator owns or is expressly authorised to test, and excludes fraud, ban evasion, anti-cheat targets and misrepresenting a device one does not control. The new surfaces hold to that with rules a reviewer can check in code:

1. **Local host only.** Every write targets the machine Phantom runs on. No surface transmits frames, packets or identifiers on behalf of another device.
2. **No impersonation on shared media.** Network-facing MACs default to random locally administered addresses, the same scheme Windows, Android and iOS use for randomisation. A real vendor OUI stays opt-in for lab profiles.
3. **No third-party device IDs.** Phantom does not edit identifiers a vendor stores to enforce its own controls (Adobe device IDs, `machineID` in `~/.claude.json`, game or anti-cheat IDs). It audits them and points at the vendor's documented privacy switch.
4. **No attestation spoofing.** TPM endorsement keys, attestation quotes and FIDO authenticators are audit-only. A forged attestation would be the misrepresentation the boundary excludes.
5. **No new detection resistance.** The new surfaces add no hiding of Phantom itself and no timing camouflage. The existing `phantom-driver/src/timing.c` jitters spoofed IOCTL replies to match real timing, which serves hiding from a detector more than privacy; the author should decide whether it stays.
6. **No TLS mimicry of a named client.** The network surface audits fingerprints and steers toward common stacks. It does not forge another product's ClientHello.

## 2. Layer map

| Surface | Main identifiers | L2-change | Needs driver or firmware | Audit-only or guidance |
|---|---|---|---|---|
| L0 / L1 hardware | SMBIOS, disk and NVMe serials, NIC MAC, GPU IDs, TPM EK, EDID, USB serials | Windows MAC through `NetworkAddress`; EDID override (opt-in) | SMBIOS (L0), disk and NVMe serials, permanent MAC, USB serials (L1) | TPM EK, GPU PCI IDs, NVML GPU UUID |
| L2 OS | MachineGuid set (shipped), ComputerName, Advertising ID, DHCPv6 DUID, Linux machine-id set (shipped) | yes, all listed | none | Windows "device ID" origin, MSA device token |
| RF and physical | Wi-Fi probes and MAC, BT address and name, DHCP options, mDNS, LLMNR, NetBIOS, PRNU, printer dots | Wi-Fi profile hygiene, multicast name services off, DHCP client ID | BT classic address (vendor HCI) | PRNU, printer dots, BT controller address |
| Network | IPv6 address, DNS, JA3/JA4, Tor, Tailscale, station protocol | IPv6 privacy, DoH | none | JA3/JA4, TCP stack fingerprint, Tailscale keys |
| Application | Browser surfaces, UA strings, app telemetry, git and gh identity, SSH key offering | config and policy files, git config, telemetry switches | none | gh public profile (web), browser canvas and WebGL values |
| Content | EXIF, XMP, PDF and docx properties, paths, usernames, timestamps, time zones | scrubber fix mode on outputs | none | PRNU in images |
| Inference | account, IP, prompt and tool content | redaction, local routing, telemetry off | none | what a signed-in model receives (section 3.7) |

## 3. Surfaces

### 3.1 Firmware and hardware (L0 and L1)

**Identifiers.** SMBIOS types 0 to 3 (BIOS vendor and version, system UUID and serial, board serial, chassis serial and asset tag), all modeled in `SmbiosIdentifiers`. Disk serial, model and firmware revision from ATA IDENTIFY, `IOCTL_STORAGE_QUERY_PROPERTY` and, for NVMe, the Identify Controller `SN` field through the protocol-specific property query. NIC permanent and current MAC. GPU PCI vendor, device and subsystem IDs, the PnP instance suffix, the driver key GUID, and the NVIDIA GPU UUID and board serial exposed through NVML. TPM manufacturer, spec version and the endorsement key (EK) public part and certificate. Monitor EDID manufacturer, product code, serial (bytes 12 to 15 and the `0xFF` descriptor) and week and year. USB `iSerialNumber`, which Windows folds into the device instance ID, plus the history Windows keeps under `Enum\USB`, `Enum\USBSTOR` and `MountedDevices`.

**Audit.** Everything above is readable today or with small additions: `GetSystemFirmwareTable('RSMB')`, storage IOCTLs including the NVMe protocol query, SetupAPI and the `Enum` tree for USB and EDID, `Get-TpmEndorsementKeyInfo` (admin) for the EK, and NVML when the NVIDIA driver is present. The validator's `sources.rs` already has the Windows readers for SMBIOS, disk, GPU and display; USB, NVMe protocol data, EK and NVML are new readers.

**Safe change at L2.**
- Windows NIC MAC through the adapter's documented `NetworkAddress` value under the network class key, then a disable and enable of the adapter. Layer 2 on Windows today writes five registry values and no MAC; this closes that gap without a driver. The privacy plan measured an existing override on Ethernet 3, so the path works on the workstation, and that adapter goes through `adopt` before Phantom writes it. Back up the prior value or its absence; revert deletes or restores it.
- EDID override through the display device's `EDID_OVERRIDE` registry block (the mechanism CRU uses; moderate confidence on exact layout per driver). Opt-in only, because it feeds the GPU driver's mode list as well as the serial readers.

**Needs a driver (L1).** Disk and NVMe serials as seen by other processes, the permanent MAC (`OID_802_3_PERMANENT_ADDRESS`), and USB serials through a filter on the device stack. The driver today has none of the NVMe protocol path and no USB filter (measured by grep), and its attach and detach IOCTLs still return `STATUS_NOT_IMPLEMENTED` (`docs/kernel-driver-review.md`).

**Needs firmware (L0).** SMBIOS at the source. A kernel rewrite of SMBIOS would patch non-exported kernel paths, which collides with PatchGuard and HVCI, so SMBIOS belongs in the DXE module.

**Audit-only.** TPM EK: fixed per TPM unless firmware issues `TPM2_ChangeEPS`, which voids the EK certificate and breaks Windows Hello and any attestation. GPU PCI IDs: the driver binds on them, so changing them stops the GPU loading. NVML UUID: read through the NVIDIA driver's private interface. Security keys and other HID authenticators are excluded from the USB filter by class.

**Breakage risks.** Windows activation keys its digital licence to a hardware hash in which the board weighs most, so an L0 SMBIOS change can trigger re-activation (moderate). Adobe and other licensing SDKs read disk serials and MACs. A DXE module only loads with Secure Boot off or with the operator's own keys enrolled; either changes PCR 7, which BitLocker binds to by default, so BitLocker asks for the recovery key (high). Some firmware measures SMBIOS into PCR 1 (moderate), which matters only for non-default BitLocker PCR profiles. An EDID override can break display calibration: ICC profile associations and Calibrate Pro key on the monitor's device instance. A new USB serial makes Windows enumerate a new device and lose per-device settings.

**Build.** (a) New read-only readers: NVMe protocol query, USB, EDID raw, EK info, NVML. 4 to 6 days. (b) Windows MAC through `NetworkAddress`, with VM and workstation tests. 2 to 3 days. (c) EDID override, opt-in. 3 days. (d) Driver: implement attach and detach, NVMe and USB filters, Driver Verifier runs on a VM that can bugcheck. 20 to 40 days, plus an EV certificate and Microsoft attestation signing (cost, unpriced here). (e) DXE: 15 to 30 days plus a test board the operator accepts losing. Estimates for (d) and (e): low confidence.

### 3.2 OS (L2, existing, plus telemetry identifiers)

**Identifiers.** Shipped: MachineGuid, HwProfileGuid, SQMClient MachineId, ProductId, InstallDate on Windows; machine-id, hostname, MAC on Linux. Added: ComputerName on Windows (modeled, not applied), the Advertising ID (`HKCU\...\AdvertisingInfo`), the DHCPv6 DUID (`Tcpip6\Parameters\Dhcpv6DUID`, which is derived once from a MAC and a timestamp and survives later MAC changes), the Windows "device ID" shown in Settings (believed to be the SQMClient MachineId; moderate), Defender and Delivery Optimization client IDs (location unknown), and on Linux `boot_id`, NetworkManager connection UUIDs and distro report IDs.

**Audit.** All registry and file values above, plus the telemetry level (`AllowTelemetry`), DiagTrack service state, CEIP and Application Experience tasks, and Recall policy.

**Safe change.** The shipped five; the DUID (moves with the profile's MAC so they stay consistent); the Advertising ID (disable and reset); the documented diagnostic and privacy switches as a `telemetry` surface, each a registry or policy value with a backup entry. ComputerName waits for the full rename path the README describes, because a registry-only write breaks `shutdown` and WMI.

**Audit-only.** The Microsoft account device token and Insider enrolment. Windows Home ignores the "off" telemetry policy level and Insider requires Optional data (privacy plan, documented high), so `audit` reports the floor the edition allows.

**Breakage risks.** Phantom's own licence. `MachineFingerprint::collect()` hashes the live MachineGuid and SQMClient MachineId, and `LicenseGuard::load()` checks the stored key against that hash. After a Layer 2 apply both values differ, so a Pro or Enterprise key no longer matches and Phantom falls back to Free; profile origin marks then verify as foreign (inferred from code, not run). Free still covers Layer 2 and revert, so it is not a lockout. The conflict grows with every layer: the fingerprint's other inputs (`SystemProductName`, `BaseBoardProduct`, and on Linux the DMI serials and machine-id) are values L0 and L2 will also write. So the rule is that the licence fingerprint never reads a live value any Phantom layer can apply. Design, build step 1:

- `MachineFingerprint::collect()` takes its inputs from the originals in the backup when a backup exists, and from the live values only when none does (the machine is in its original state, so they are the originals).
- The backup gains a keyed MAC over its original-value entries, written at first apply, so editing the backup to move a key to another machine fails verification and falls back to Free, the same way a failed state MAC does in `guard.rs` today.
- Profile origin marks use the same original-value fingerprint, so a profile stays "local" while it is applied.
- A test applies a profile in a sandbox, then asserts `LicenseGuard::load()` keeps its tier and `check_origin` still returns `Local`.

Rejected: dropping MachineGuid and MachineId from the hash. Every remaining input is writable at some layer, so the conflict returns at L0. Adobe and similar SDKs may re-activate on a MachineGuid change. DPAPI keys derive from the user's credentials, not MachineGuid, so stored secrets survive (moderate).

**Build.** Licence fingerprint fix, 1 to 2 days. DUID and Advertising ID, 2 days. Telemetry surface (catalogue, audit, apply, revert), 4 to 6 days. ComputerName full rename, 3 to 5 days.

### 3.3 Physical and RF

**Identifiers.** Wi-Fi: the probe requests the station sends, which carry the MAC and, for saved hidden networks, the SSID; the saved-network list itself; the per-network MAC. Bluetooth: the classic BD_ADDR (fixed in the controller), BLE advertising addresses, and the device name, which follows the computer name on Windows (moderate). DHCP: option 12 hostname, option 61 client identifier (Windows sends the MAC), the DHCPv6 DUID. LAN name services: mDNS, LLMNR, NetBIOS over TCP/IP, SSDP and WS-Discovery announcements, all of which broadcast the machine name. Physical: camera sensor noise (PRNU), which ties every photo to one sensor, and the yellow tracking dots many colour laser printers add.

**Audit.** Saved Wi-Fi profiles and which ones are marked to connect when not broadcasting (`netsh wlan show profiles`), the live Wi-Fi MAC against the permanent one, the random hardware address setting per network, Bluetooth radio state, discoverability and name, the DHCP options this host sends, and the state of each name service. On Linux, the matching NetworkManager keys (`wifi.cloned-mac-address`, `wifi.scan-rand-mac-address`, `ipv4.dhcp-send-hostname`, `ipv4.dhcp-client-id`, `ipv6.dhcp-duid`) and `systemd-resolved` and Avahi state.

**Safe change.** Export each Wi-Fi profile before any edit (`netsh wlan export profile`), then clear the not-broadcasting flag or remove stale profiles; revert re-imports. Turn off LLMNR (`EnableMulticast=0` under the DNS client policy key), NetBIOS per interface (`NetbiosOptions=2`), the mDNS responder (`EnableMDNS=0`, moderate), and the discovery publication service. Set the DHCP client ID from the profile. On Linux, write the NetworkManager keys in a drop-in file that revert deletes. The Windows per-network random address setting is applied through its documented UI path and verified by reading the live MAC; its storage location is undocumented (low confidence), so Phantom does not write it directly.

**Needs firmware.** The Bluetooth classic address changes only through a vendor HCI command or controller firmware. Out of scope.

**Audit-only and guidance.** PRNU cannot be removed in software; downscaling and recompression weaken it without a guarantee, so `audit --surfaces content` flags images from the operator's own cameras and the guidance is to publish from a dedicated camera or not at all. Printer dots: guidance to print monochrome or on a printer known not to add them, and to check a printout under blue light.

**Breakage risks.** Removing a Wi-Fi profile loses its password until re-entered (the export holds it in clear, so the backup file is ACL'd to SYSTEM and Administrators). Disabling NetBIOS and LLMNR breaks name lookup of old LAN devices by short name. A new DHCP client ID gets a new lease and may change the LAN IP that port forwards point at. None of these touch activation, Tailscale, sign-in or BitLocker.

**Boundary.** Phantom only changes what this host emits. It sends no frames as another station, no deauthentication, no spoofed beacons.

**Build.** RF audit, 3 days. Name-service and DHCP apply, 3 to 4 days. Wi-Fi profile hygiene with export and import, 2 days.

### 3.4 Network

**Identifiers.** The public IPv4 address and the IPv6 /64 prefix (both seen by every remote host; the prefix is a stable household identifier), the IPv6 interface ID, DNS queries in clear, the TLS ClientHello fingerprint (JA3, JA4) of each client stack, the TCP and IP stack fingerprint (window size, options, TTL), Tor circuit use, and Tailscale's node name, node key, machine key, account and host info.

**Audit.** IPv6 privacy state and the temporary address in use; configured resolvers and whether DoH applies; Tor SOCKS reachability and which apps route through it; `tailscale status --json` for the hostname and host info sent to the coordination server. For JA3 and JA4, a local loopback TLS listener records the ClientHello of each installed client stack (browser, Schannel, rustls, OpenSSL, Go, Python) and computes the hashes locally. No third-party fingerprinting site is contacted.

**Safe change.** IPv6 privacy extensions and randomised interface IDs (already on at the workstation, measured). DNS servers and the DoH "encrypted only" setting, with the prior servers backed up. Tailscale node name through `tailscale set --hostname`, recorded as the prior name for revert.

**Audit-only.** JA3 and JA4 come from each client's TLS library; the lever is choosing a common stack, which `audit` ranks. Tailscale node and machine keys: Phantom never edits Tailscale state, because a new machine key is a new node with new ACL identity. The public IP: the lever is the route (VPN, Tailscale exit node, Tor), which belongs to the station protocol.

**Breakage risks.** Changing the route or exit changes the source IP that GitHub, Google and Anthropic see, which can prompt device verification on web sign-in (moderate). Strict DoH breaks captive portals and router-local names. A Tailscale rename changes MagicDNS names that scripts use. NordVPN over Tailscale can push Tailscale to DERP relays.

**Station protocol.** Phase 2 requirements and their mapping are in section 4.

**Build.** Network audit with the local ClientHello recorder, 4 to 5 days. DNS and IPv6 apply, 2 days.

### 3.5 Application

**Identifiers.** Browser surfaces: User-Agent and UA client hints, canvas and WebGL renderer strings (the RTX 4090 is visible to every site through WebGL, privacy plan measured), AudioContext output, installed fonts, screen and window sizes, time zone, languages, `hardwareConcurrency`, `deviceMemory`, WebRTC local addresses, and the signed-in browser account. Our own tools' User-Agent and `mailto` strings (30 scripts send the personal gmail, privacy plan section 1.5). App telemetry: Claude Code, Codex, Windows, NVIDIA, MSI, Adobe, Syncthing, Firefox. Git: `user.name`, `user.email`, commit time zone, signing key. gh: the public profile fields. SSH: the client offers every loaded public key to every server, and GitHub publishes each user's keys, so any SSH server can name the user.

**Audit.** A local page on `127.0.0.1` collects the browser values and writes them to the Phantom data directory. Config files: Firefox `prefs.js` and `policies.json`, Chrome policy keys, Claude Code `settings.json` env, Codex `config.toml`, Syncthing `config.xml`. A source scan for UA and `mailto` strings. `git config` at every level and per repo, the commit offset distribution, `gh api user`, and `ssh -G <host>` for `IdentitiesOnly`.

**Safe change.** Firefox fingerprinting protection and telemetry prefs through `policies.json`; Chrome policies such as WebRTC IP handling; Claude Code and Codex telemetry switches (section 3.7); per-repo git email and a UTC commit alias; `IdentitiesOnly yes` per SSH host. Every file is copied into the backup before the edit. Phantom favours uniformity over randomisation in browsers, since a randomised canvas makes a browser rarer, and recommends Tor Browser for work that must not carry the IP.

**Audit-only and guidance.** The gh public profile and email privacy settings are web settings; Phantom prints the steps. Canvas and WebGL values change only with the browser.

**Breakage risks.** A git email change can make GitHub reject pushes when "block command line pushes that expose my email" is on and the address is wrong. Browser policy can break sites that rely on WebRTC. Telemetry switches can disable feature-flagged features (Claude Code docs, privacy plan). Sign-in for Claude, Codex and gh is token-based and not touched.

**Build.** App audit including the local fingerprint page, 4 to 5 days. Config apply with backup for the listed files, 4 days.

### 3.6 Content and metadata (pre-publish scrubber)

**Identifiers.** EXIF and XMP (camera serial, GPS, software, owner), IPTC, PNG text and `eXIf` chunks, PDF `/Author`, `/Creator`, `/Producer`, dates and XMP, docx `docProps/core.xml` creator and `lastModifiedBy` and `custom.xml`, embedded absolute paths (`C:\Users\Zain`, `C:\dev`) and usernames, the machine name, the tailnet name, timestamps and time zone offsets. Privacy plan census: 150 tracked files with the user path, 753 with `C:\dev`, 55 with the personal gmail, 11 PDFs and 12 docx files with an author set.

**Design.** The check lives in Flywheel (`scripts/check_publish_metadata.py`), as the privacy plan proposes, and runs from `publish_public.py` and as a CI step in the site build. Phantom's part is the deny list: `phantom audit --export-denylist` writes the live identifiers this machine has (serials, MACs, machine name, user name, local addresses, tailnet name) as salted hashes plus the known string patterns, into the Phantom data directory. The scrubber matches tokens against the hashes, so raw serials never sit in a committed file or a CI log. Each class has a `check` mode that fails with file and line, and a `fix` mode that rewrites a copy: strip image metadata but keep ICC colour data, blank PDF and Office author fields and pin dates to `SOURCE_DATE_EPOCH`, replace paths with repo-relative ones. Git checks warn on a commit to be pushed with a non-noreply email or a non-UTC offset.

**Boundary.** The scrubber only rewrites the operator's own outputs before publication.

**Breakage risks.** False positives on fixtures that need paths (an allow list per repo). The intended name on the CV stays, so the byline is configurable. Rewriting already-pushed history is out of scope; the plan's "fix forward" stands.

**Build.** Deny-list export in Phantom, 2 days. Scrubber in Flywheel with check and fix modes and the site CI hook, 5 to 7 days.

### 3.7 Inference boundary

**What a signed-in hosted model always receives.** The account (email, name, account and organisation IDs, billing tier), the source IP of each connection or the VPN exit, and the full text of every prompt, system prompt, instruction file, memory file, tool call and tool result. That includes every file path and every byte a tool reads. Client version, model, timing and request sizes go with it. Telemetry switches do not change any of this; only what is sent does.

**Levers.**
1. **Redact before the prompt, with its own filter.** The workspace's `safe_*` membrane does no identifier redaction. The privacy plan read its hooks and found vocabulary substitution and framing, with no redaction function (moderate), so it is not a privacy layer and this design does not count on it or edit it. The inference surface gets a separate filter: a shared redaction module (the plan proposes `harness/redact.py` in Flywheel, used by the scrubber too) driven by Phantom's deny list, called from a Claude Code `UserPromptSubmit` hook on prompts and a `PostToolUse` hook on tool results, plus a `PreToolUse` rule that blocks reads of files marked sensitive. It swaps each match for a stable placeholder (`~` for the user path, `HOST`, `EMAIL`, `ID-1`), so the model can still reason about structure. Whether a `PostToolUse` hook can rewrite a tool result, or only block it, depends on the Claude Code version (unknown); where it can only block, the filter blocks and points at the redacted copy it wrote. A redacted value the task needs makes the task fail, and the hook says so. The filter preserves meaning; it changes identifiers and nothing else, so evidence work stays checkable.
2. **Keep raw data local.** Raw audits, backups, captures and the deny list stay in the Phantom data directory; prompts get masked summaries.
3. **Local models for sensitive or bulk steps.** Bulk reading, classification and anything over raw identifiers route to the local model servers (the relay and local-model tools in this workspace) and only conclusions reach the hosted model.
4. **Telemetry off.** For Claude Code: `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`, `DISABLE_ERROR_REPORTING=1`, `DISABLE_FEEDBACK_COMMAND=1`, and `skipWebFetchPreflight: true` for the WebFetch hostname check. For Codex: `[analytics] enabled = false`, `[otel] metrics_exporter = "none"`, `[feedback] enabled = false`. Values from the privacy plan, which read the vendor docs on 2026-11-04. Phantom's `infer` surface audits and applies them with a backup of each file.
5. **Gateway for API clients.** The unmerged `phantom boundary` branch is a sealed-policy loopback gateway with content-free receipts. It now allows only exact-byte approved bodies to a loopback upstream. Making it a redacting proxy to a hosted API is a scope change the branch rules out on purpose, so it needs its own design review before merge.

**Breakage risks.** None to sign-in. Feature-flagged features may switch off. Over-redaction lowers answer quality; the hook logs each redaction (content-free) so the cost is visible.

**Boundary.** Redaction removes the operator's own data from the operator's own requests. It does not alter the request's nature or get around provider policy.

**Build.** Telemetry `infer` surface, 2 days. Redaction hook and membrane step, 4 to 6 days. Gateway review and extension, 8 to 12 days.

## 4. Station protocol: phase 2 requirements

Source: section 6 of the privacy plan, which sets nine properties for the station's own networking protocol, and its section 6.3, which places it in Phantom as a new crate (`phantom-net`) beside `phantom-svc`. This design adopts them. The table says which Phantom surface supplies each property and what is new.

| Plan property | Supplied by | New work |
|---|---|---|
| 1. Identifier minimisation on the link: per-network MAC, stable on one network and different across networks; generic DHCP client ID and hostname; no LLMNR, NetBIOS, mDNS or SSDP on untrusted links; random IPv6 interface IDs or IPv6 off | RF surface (3.3), Windows MAC (3.1), DUID (3.2) | Per-network MAC derived as HMAC(profile seed, network ID), locally administered; applied on each network join by `phantom-svc` |
| 2. Fail closed, enforced by Windows Filtering Platform, never by the transport's client; captive portal in an isolated browser | `phantom-net` | WFP filter set: default block on the physical interface, permits only for the tunnel endpoints and the portal session; removed on revert |
| 3. Transport selection, pinned per traffic class: Tor for anonymous fetches and station onion services, WireGuard or Tailscale to a controlled exit for signed-in work, plain only by explicit allow; signed-in services never over Tor exits | `phantom-net` policy | Class table keyed by process and destination; each flow tagged with its class in the log |
| 4. DNS inside the tunnel only | Network surface (3.4) | WFP block on port 53 and 853 to anything but the tunnel resolver |
| 5. Padding and timing: bucketed sizes, cover traffic, batched non-interactive fetches | `phantom-net` | Optional and costed: bucket padding first, cover traffic last, each measured for bandwidth and latency cost |
| 6. One neutral User-Agent and TLS client profile for station tools; no contact emails or locale hints | App surface (3.5), JA3/JA4 audit (3.4) | Shared station HTTP client config; the scrubber's fetch-code check enforces it |
| 7. Separate exit and circuit per account identity | `phantom-net` policy | Tor SOCKS auth isolation per class; one WireGuard exit per signed-in identity |
| 8. Every flow logged locally; join-time self-test that refuses to open the gate on a leak | Audit (all surfaces) | Automated version of the plan's capture: diff what left the physical interface against the policy and the deny list, content-free receipts to Flywheel |
| 9. Defined failure modes: block on transport loss, Tor bridges when Tor is blocked, existing peers only when Tailscale coordination is down, clear failure on clock skew, portal then re-close | `phantom-net` | Each mode is a test case in the join self-test |

Verbs: `audit --surfaces net` reports the live link identity, active WFP filters and the last self-test; `apply` installs the filter set and the class table after backing up any existing WFP filters Phantom did not create (it never deletes those); `validate` runs the self-test; `revert` removes Phantom's filters and restores the link identity. If revert fails, the filters stay and the machine stays blocked, which is the fail-closed choice; `phantom revert --force-open` removes them by filter key.

Two design constraints added here: use maintained implementations (WireGuard, Tor, TLS 1.3, QUIC, Noise) and write no new cryptography; and build the custom wire format only where the self-test shows a gap the composed protocols leave, since the plan's 6.2 finds most properties already available and the new work in the policy layer.

Breakage: a fail-closed gate blocks everything when the tunnel is down, including Windows Update, licence checks and sign-in; a WFP mistake can cut the machine off the network until `--force-open`. Tailscale traffic must be pinned to its own class or it fights the gate.

Effort: policy layer, WFP gate and self-test 12 to 18 days; padding 3 to 5 more; a custom transport 15 to 25 more if the self-test justifies it (low confidence).

## 5. Cross-cutting breakage risks

| Risk | Triggered by | Mitigation |
|---|---|---|
| Phantom's own licence | Any L2 apply on Windows (live MachineGuid and SQMClient MachineId feed the fingerprint) | Build step 1: fingerprint from the MAC'd backup originals |
| Third-party licence re-activation | MachineGuid, MAC, disk serial, SMBIOS | Deactivate Adobe and similar first; test on a VM; keep revert ready |
| Windows activation | L0 SMBIOS board fields; L1 disk serial (moderate) | Microsoft-account-linked licence and the hardware-change troubleshooter; test on a VM |
| Tailscale node identity | Never touched by Phantom; renames are cosmetic | Rename only through `tailscale set`; never edit its state file |
| Claude and GitHub sign-in | Route and exit changes (new IP), not identifier changes | Expect a device check on web sign-in; tokens untouched |
| BitLocker and TPM sealing | DXE load (Secure Boot change, PCR 7); TPM clear; disk signature or GPT GUID change (boot entries) | Suspend BitLocker for one reboot before an L0 step; never clear the TPM; boot identifiers stay audit-only |
| Display calibration | EDID override | Opt-in; warn when an ICC profile is associated |

## 6. Build order

| Step | Work | Effort (days) | Why here |
|---|---|---|---|
| 1 | Licence fingerprint reads MAC'd backup originals, plus the `adopt` path for foreign overrides | 2 to 3 | Removes a self-inflicted break before anything else applies |
| 2 | `audit --surfaces all`, read-only, with masked output and deny-list export | 10 to 14 | Census first; it feeds the scrubber and redaction |
| 3 | Content scrubber in Flywheel and site CI | 5 to 7 | Largest measured leak (paths, gmail, PDF and docx authors), no system risk |
| 4 | `infer` and `telemetry` surfaces: switches with backup | 6 to 8 | Cheap, reversible, cuts optional traffic |
| 5 | Inference redaction hook and membrane step | 4 to 6 | Lowers what reaches the hosted model |
| 6 | Windows MAC through `NetworkAddress`, DUID, DHCP client ID, name services, Wi-Fi hygiene | 8 to 10 | RF and LAN identity with L2 writes only |
| 7 | App config apply (browser policy, git, SSH) and DNS and IPv6 apply | 6 | Mostly config files |
| 8 | `phantom-net`: WFP gate, class table, join self-test (plan section 6, properties 1 to 4 and 6 to 9) | 12 to 18 | Needs steps 2, 6, 7 |
| 9 | Boundary gateway review and extension | 8 to 12 | Scope change needs review |
| 10 | Driver functional work and signing | 20 to 40 plus signing cost | Blocked on EV certificate and attestation signing |
| 11 | DXE SMBIOS | 15 to 30 plus a test board | Highest risk: Secure Boot, BitLocker, activation |

Steps 1 to 9 total about 61 to 84 days of focused work (moderate confidence). Steps 10 and 11 are low confidence and cost-gated.

## 7. Open questions for the author

1. Does `timing.c` stay in the driver, given rule 5 in section 1.1?
2. The standing rule that agent web work runs in the signed-in `zaindharper` Chrome profile joins the Google account to every agent page view. Keep it, or give agents a signed-out profile?
3. The privacy plan places the station protocol in Phantom as `phantom-net`. Confirm that, under the rule that tools land as packages in an existing home.

## 8. Merge slot: workstation privacy plan

The plan at `C:/dev/mission-control/2026-11-04-workstation-efficiency/PRIVACY-PLAN.md` (phase 1, cleanroom workstation) is merged. Used here: the identifier census (1.1), network identity (1.2), app and account identity (1.3), the metadata census (1.4), User-Agent leaks (1.5), the Claude boundary (1.6), where Phantom fits (3, including the licence conflict and the Ethernet 3 override), the scrubber (4), inference-boundary minimisation and the membrane finding (5), the phase 2 protocol requirements (6) and what cannot be hidden (7). Its cleanroom steps 1 to 12 map to build steps 3 to 7. If the plan changes, re-read sections 3, 5 and 6 first, since sections 3.2, 3.7 and 4 here depend on them.

## Verified

- Licence fingerprint inputs: read in `phantom-license/src/fingerprint.rs` and `guard.rs`. The fallback to Free after an apply is inferred from that code; not run.
- Driver gaps (no NVMe protocol path, no USB filter, attach and detach unimplemented): grep over `phantom-driver/src` and `docs/kernel-driver-review.md`.
- Boundary gateway scope: read from `docs/boundary-gateway.md` on `codex/phantom-boundary-gateway-20260909`.
- Membrane finding (no identifier redaction in the `safe_*` hooks), the Ethernet 3 override and the phase 2 protocol properties: taken from the privacy plan; the membrane was not re-read here.
- Workstation values and vendor telemetry controls: taken from the privacy plan, which measured or read them on 2026-11-04; not re-measured here.
- Registry locations and protocol behaviour not in the plan carry their confidence word beside them. Effort figures are estimates.
