# AI IP 1.1 modifications to OpenAI Codex

AI IP 1.1 is a modified derivative of OpenAI Codex. The fork preserves the complete upstream Git ancestor and records each product-owned patch instead of treating a copied source snapshot as provenance.

Locked upstream commit: `bf5ebd98c567931d82e873a4afdac7548bd85979`.

Original fork point: `4ef1d4b89bd419c976b04fefa0fd36844e898340`. The fork is re-synced onto newer upstream commits periodically; `.ai-ip/upstream.lock.toml` records both the immutable fork point and the current synced upstream.

Product origin status at this checkpoint: `unconfigured`.

DeerFlow is not a runtime dependency or fallback.

## Source and license policy

- The fetch remote named `upstream` points to `https://github.com/openai/codex.git`; its configured push URL is deliberately unusable.
- `origin` remains absent until a real product fork repository is selected and verified. A local placeholder URL is not provenance.
- The imported `LICENSE`, root `AGENTS.md`, and upstream portion of `NOTICE` remain intact.
- This source-import checkpoint records source governance only. A distributable SBOM and artifact-specific third-party notices are generated later for each actual release candidate.

## Patch policy

Every fork-specific patch is classified as preserve, directly modify, replace, or add. Each entry states the business reason, upstream seam, tests, sync risk, and deletion or rollback path. Business capability may justify a deep fork, but undocumented edits to the execution substrate are not allowed.

All fork-specific patches are recorded in `docs/architecture/codex-fork-patch-ledger.md`.

## Current boundary

Phase 0A.1 adds only provenance verification and governance files. It does not modify the Codex runtime, close G0, prove the native macOS or Windows baseline, prove business quality, or authorize Plan 02.
