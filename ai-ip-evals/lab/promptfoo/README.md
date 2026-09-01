# Promptfoo adapter dependency

This private workspace pins Promptfoo exactly at `0.122.0`. It is a replaceable
Codex App Server adapter, not a grader and not a release authority.

The host-side Python adapter only renders path-neutral configuration, parses a
bounded result, and compiles immutable Task 4 launch specifications. Task 4
retains ownership of the candidate process group, file descriptors, deadline,
termination, metering, and launch attestation. The sealed runner launched in
that process group may invoke this local CLI; candidate App Servers never
receive review material, arm mappings, release authority, or provider secrets.

Use a fresh Promptfoo state directory when checking the pin so the check neither
reads nor mutates user-level Promptfoo state:

```sh
HOME=/path/to/fresh/home \
PROMPTFOO_CONFIG_DIR=/path/to/fresh/config \
PROMPTFOO_CACHE_PATH=/path/to/fresh/cache \
npx --yes pnpm@10.34.5 --dir ai-ip-evals/lab/promptfoo exec promptfoo --version
```

The expected output is `0.122.0`, and Node.js must be at least `22.22.0`.
