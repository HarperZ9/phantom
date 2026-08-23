# Linux VM dogfood: MAC persistence across a reboot

The one Linux guarantee that needs real hardware is that a spoofed MAC returns
after a reboot. machine-id and hostname are file-based and persist on their own,
and the rootful CI job (`scripts/linux-apply-integration.sh`) exercises them on
every push. The MAC is different: the NIC comes up on its hardware address after
a reboot, and `phantom.service` has to reapply the active profile to restore the
spoof. A network namespace has no physical NIC, so this cannot be tested in CI;
it needs a real boot with a real interface. This is that test.

## Result

PASS. On a VirtualBox Ubuntu 24.04 guest, a spoofed MAC survived a power-cycle
because `phantom.service` reapplied it at boot.

| Identifier | Hardware / original | After apply (spoofed) | After power-cycle |
|---|---|---|---|
| MAC (`enp0s3`) | `08:00:27:92:8e:d5` | `00:24:d7:e8:ef:6f` | `00:24:d7:e8:ef:6f` |
| machine-id | `e42d3ab1…b9d253` | `7715b534…7f8699` | `7715b534…7f8699` |
| hostname | `phantom-guest` | `LAB-G9UPRWB` | `LAB-G9UPRWB` |

The boot journal shows the reapply running before the network came up:

```
systemd[1]: Starting phantom.service - Phantom hardware identity privacy...
phantom-svc[642]: Phantom: reapplied 'vm' (5 identifiers).
systemd[1]: Finished phantom.service - Phantom hardware identity privacy.
```

The spoofed MAC and the expected machine-id and hostname are deterministic from
the profile seed, so the values above are checkable: generating the same profile
(`profile generate vm --seed phantom-vm-dogfood`) reproduces
`00:24:d7:e8:ef:6f`, `7715b534…`, and `LAB-G9UPRWB`.

## Uninstall restores the true identity

PASS. Removing the package reverts every spoofed identifier before the binaries
go, the Sev-1 bar. After `apt remove phantom` and a reboot:

| Identifier | Value after uninstall | Original |
|---|---|---|
| MAC (`enp0s3`) | `08:00:27:92:8e:d5` | `08:00:27:92:8e:d5` |
| machine-id | `e42d3ab1…b9d253` | `e42d3ab1…b9d253` |
| hostname | `phantom-guest` | `phantom-guest` |

`phantom` and `phantom-svc` are gone and `phantom.service` reports `not-found`.
Profiles and the license under `/var/lib/phantom` are left in place so a
reinstall picks the setup back up.

## Environment

- VirtualBox guest, `Ubuntu_64`, Ubuntu 24.04 server cloud image, provisioned
  with a cloud-init NoCloud seed (an injected SSH key). Disk and image live off
  the system drive.
- One NAT adapter. In the guest it is `enp0s3`, an e1000 PCI device with a
  `/sys/class/net/enp0s3/device` link, so `phantom` treats it as physical and
  spoofs it. The hardware MAC VirtualBox assigns (`08:00:27:…`) is what the NIC
  resets to on boot.
- The guest is driven over SSH through a host port-forward to guest `:22`.

## Procedure

1. Boot the guest, confirm `enp0s3` is physical and record its hardware MAC.
2. Install the `.deb`. It enables `phantom.service` (reapply-on-boot) but changes
   nothing yet.
3. `phantom profile generate vm --seed phantom-vm-dogfood`, then `phantom apply
   vm`. Apply spoofs the MAC, machine-id, and hostname. Changing the MAC bounces
   the interface, so the current SSH session drops and reconnects. Run apply
   detached (`systemd-run`) so it survives the bounce.
4. Power-cycle the guest (a clean ACPI shutdown, then boot).
5. Read `enp0s3`'s MAC. It is the spoofed value, restored by `phantom.service` at
   boot, not the hardware MAC.
6. Remove the package and power-cycle again. The removal hook reverts the
   identity, so the MAC, machine-id, and hostname return to their originals.

## Notes

- On the first power-cycle the guest stalled once in the initramfs (an fsck after
  an unclean-looking shutdown). A second clean power-cycle booted normally. This
  is a VirtualBox and power-cycle artifact, unrelated to `phantom`.
- Changing a MAC bounces the interface, so a live SSH or other connection over
  that interface drops for a moment and has to reconnect. After a clean boot the
  interface comes up with the spoofed MAC and DHCP succeeds normally.
