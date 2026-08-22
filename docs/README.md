# Phantom documentation

## For users

Start with the [user guide](user/README.md):

- [Install](user/install.md)
- [Your first profile](user/first-profile.md)
- [Licensing](user/licensing.md) and [activation](user/activate.md)
- [Privacy and phone-home](user/privacy.md)
- [Uninstall](user/uninstall.md)
- [Verifying your download](signature-verification.md)

## For operators (fleet deployment)

- [Deployment](deployment.md): silent install, `PHANTOM_DATA_DIR`, and
  centralized config.
- [Windows runbook](windows-runbook.md): a reproducible Layer-2 apply,
  validate, and revert on a fresh image.
- [JSON API](api.md): the stable machine-readable envelope for scripting.

## For the vendor (licensing operations)

- [Issuance workflow](issuance-workflow.md): issuing and revoking keys.
- [Master seed rotation](master-seed-rotation.md): the signing-seed
  precedence and rotation discipline.

## Release engineering

- [Release dogfood checklist](rc1-dogfood.md): the end-to-end
  customer-flow rehearsal every release runs before its tag is promoted.
- [MSI install runbook](msi-install-runbook.md): per-artifact install QA.

## Project history

- [Phase 1 sprints](phase-1-sprints.md): the sprint record behind v1.0.0.
