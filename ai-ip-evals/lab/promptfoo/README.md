# Promptfoo adapter dependency and runtime seal

This private workspace pins Promptfoo exactly at `0.122.0`. It is a replaceable
Codex App Server adapter, not a grader and not a release authority.

The host-side Python adapter only renders path-neutral configuration, parses a
bounded result, and compiles immutable Task 4 launch specifications. Task 4
retains ownership of the candidate process group, file descriptors, deadline,
termination, metering, and launch attestation. The sealed runner launched in
that process group reconstructs and invokes only the sealed CLI closure;
candidate App Servers never receive review material, arm mappings, release
authority, or provider secrets.

## Reviewed Darwin x86_64 closure

The committed `runtime-manifests/darwin-x86_64.json` is the reviewed authority
for one platform-specific deployment. It binds the repository `package.json`
and `pnpm-lock.yaml`, portable Node `v24.19.0`, Promptfoo `0.122.0`, every
runtime-tree file, the deterministic archive, and all 29 launch-artifact
chunks. The closure contains 53,634 regular files and 1,632,192,483 unpacked
bytes. Unsupported operating-system/architecture pairs have no manifest and
fail closed.

The seal was built in a fresh clean-room deployment using exact pnpm
`10.34.5`, the committed lockfile, and the repository `.npmrc` hoisted linker.
No global or user Promptfoo installation was used. The audit ran the sealed
entrypoint with portable Node and isolated `HOME`, `XDG_*`,
`PROMPTFOO_CONFIG_DIR`, and `PROMPTFOO_CACHE_PATH` state. Both `--version`
(`0.122.0`) and validation of the static Codex provider configuration exited
successfully.

The audit source root
`/tmp/07b-promptfoo-full-hoisted-clean.DE9xxS/deploy` is recorded only as
machine-local build provenance. Production neither trusts nor executes that
pathname: it selects the exact platform manifest, remeasures the repository
package and lock commitments, seals the portable Node payload and all closure
chunks into Task 4 launch material, and executes only the reconstructed sealed
bytes. Any byte, digest, platform, version, count, or bound mismatch fails
closed.

Promptfoo requires Node.js `>=22.22.0`; the reviewed portable runtime is
`v24.19.0`.
