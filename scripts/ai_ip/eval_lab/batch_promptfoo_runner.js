"use strict";
const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
const { spawnSync } = require("node:child_process");
const { Readable } = require("node:stream");
const { createGunzip } = require("node:zlib");
const supportPath = require.main === module && process.argv[2]
  ? process.argv[2] : path.join(__dirname, "batch_promptfoo_result.js");
const {
  RunnerError, boundedFile, descriptorDigest, ensureParents, isolatedDirectory, isolatedOutput,
  nodeVersionSupported, safeRelative, sameIdentity, sha256, writeAll, writeAllAt,
} = require(supportPath);
const MAX_CHUNK_BYTES = 16 * 1024 * 1024;
const MAX_MANIFEST_BYTES = 1024 * 1024;
const MAX_CONFIG_BYTES = 1024 * 1024;
const MAX_RESULT_BYTES = 8 * 1024 * 1024;
const MAX_HEADER_BYTES = 4096;
const MAX_CHUNKS = 29;
const MAX_FILES = 100000;
const MAX_EVENTS = 1024;
function boundedJson(payload, label) {
  let depth = 0;
  let nodes = 1;
  let inString = false;
  let escaped = false;
  for (const byte of payload) {
    if (inString) {
      if (escaped) escaped = false;
      else if (byte === 0x5c) escaped = true;
      else if (byte === 0x22) inString = false;
      continue;
    }
    if (byte === 0x22) inString = true;
    else if (byte === 0x5b || byte === 0x7b) {
      depth += 1;
      nodes += 1;
      if (depth > 64) throw new RunnerError(`${label} exceeds JSON depth`);
    } else if (byte === 0x5d || byte === 0x7d) {
      depth -= 1;
      if (depth < 0) throw new RunnerError(`${label} has invalid JSON shape`);
    } else if (byte === 0x2c || byte === 0x3a) {
      nodes += 1;
      if (nodes > 10000) throw new RunnerError(`${label} exceeds JSON nodes`);
    }
  }
  if (inString || depth !== 0) throw new RunnerError(`${label} has invalid JSON shape`);
  try {
    return JSON.parse(payload.toString("utf8"));
  } catch (error) {
    throw new RunnerError(`${label} is not valid JSON`, { cause: error });
  }
}
function object(value, label) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new RunnerError(`${label} must be an object`);
  }
  return value;
}
function exactKeys(value, keys, label) {
  if (Object.keys(value).sort().join("\0") !== [...keys].sort().join("\0")) {
    throw new RunnerError(`${label} has invalid fields`);
  }
}
function promptfooEnvironment() {
  const names = [
    "AI_IP_ATTEMPT_ID", "AI_IP_CASE_PATH", "AI_IP_CODEX_PATH", "AI_IP_CONFIG_PATH",
    "AI_IP_PROFILE_PATH", "AI_IP_PROMPTFOO_PATH", "AI_IP_PROTOCOL_PATH",
    "AI_IP_RESULT_FD", "AI_IP_ROUTE_PATH", "AI_IP_SCHEMA_PATH", "AI_IP_TELEMETRY_FD",
    "APPDATA", "AWS_PROFILE", "CODEX_HOME", "COMSPEC", "FORCE_COLOR", "HOME",
    "HOMEDRIVE", "HOMEPATH", "LANG", "LANGUAGE", "LC_ALL", "LC_COLLATE", "LC_CTYPE",
    "LC_MESSAGES", "LC_MONETARY", "LC_NUMERIC", "LC_TIME", "LOCALAPPDATA", "NO_PROXY",
    "PATH", "PATHEXT", "PROMPTFOO_CACHE_ENABLED", "PROMPTFOO_CACHE_PATH",
    "PROMPTFOO_CONFIG_DIR", "PROMPTFOO_DISABLE_SHARING", "PROMPTFOO_DISABLE_TELEMETRY",
    "PROMPTFOO_DISABLE_UPDATE", "PROMPTFOO_OUTPUT_PATH", "SHELL", "SYSTEMROOT", "TEMP",
    "TMP", "TMPDIR", "USERPROFILE", "VOLCENGINE_PROFILE",
  ];
  for (const name of [
    "PROMPTFOO_DISABLE_SHARING",
    "PROMPTFOO_DISABLE_TELEMETRY",
    "PROMPTFOO_DISABLE_UPDATE",
  ]) {
    if (process.env[name] !== "1") throw new RunnerError(`${name} must be sealed off`);
  }
  const environment = Object.fromEntries(
    names.filter((name) => process.env[name] !== undefined).map((name) => [name, process.env[name]]),
  );
  environment.AI_IP_WORKSPACE = process.cwd();
  return environment;
}
class RecordExtractor {
  constructor(root, manifest) {
    this.root = root;
    this.manifest = manifest;
    this.pending = Buffer.alloc(0);
    this.header = null;
    this.output = -1;
    this.remaining = 0;
    this.treeHash = crypto.createHash("sha256");
    this.fileCount = 0;
    this.unpackedBytes = 0;
    this.inflatedBytes = 0;
    this.maximumInflatedBytes =
      manifest.unpackedBytes + manifest.fileCount * (MAX_HEADER_BYTES + 4) + 4;
    this.finished = false;
    this.inventory = [];
    this.identities = new Map();
  }
  consume(chunk) {
    this.pending = this.pending.length ? Buffer.concat([this.pending, chunk]) : chunk;
    if (this.header === null && this.pending.length >= 4) {
      const declaredHeader = this.pending.readUInt32BE(0);
      if (declaredHeader > MAX_HEADER_BYTES) {
        throw new RunnerError("runtime header exceeds its byte bound");
      }
    }
    this.inflatedBytes += chunk.length;
    if (this.inflatedBytes > this.maximumInflatedBytes) {
      throw new RunnerError("runtime inflated stream exceeds its byte bound");
    }
    while (this.pending.length) {
      if (this.finished) throw new RunnerError("runtime archive has trailing bytes");
      if (this.header === null) {
        if (this.pending.length < 4) return;
        const lengthBytes = this.pending.subarray(0, 4);
        const length = lengthBytes.readUInt32BE(0);
        if (length === 0) {
          this.treeHash.update(lengthBytes);
          this.pending = this.pending.subarray(4);
          this.finished = true;
          continue;
        }
        if (length > MAX_HEADER_BYTES) {
          throw new RunnerError("runtime header exceeds its byte bound");
        }
        if (this.pending.length < 4 + length) return;
        const headerBytes = this.pending.subarray(4, 4 + length);
        const header = object(boundedJson(headerBytes, "runtime header"), "runtime header");
        exactKeys(header, ["mode", "path", "sha256", "size"], "runtime header");
        const parts = safeRelative(header.path);
        if (
          !Number.isSafeInteger(header.size) ||
          header.size < 0 ||
          header.size > this.manifest.maximumFileBytes ||
          ![0o600, 0o700].includes(header.mode) ||
          !/^[0-9a-f]{64}$/.test(header.sha256)
        ) {
          throw new RunnerError("runtime archive file declaration exceeds its bound");
        }
        this.fileCount += 1;
        this.unpackedBytes += header.size;
        if (
          this.fileCount > this.manifest.fileCount ||
          this.unpackedBytes > this.manifest.maximumUnpackedBytes
        ) {
          throw new RunnerError("runtime archive expanded content exceeds its bound");
        }
        ensureParents(this.root, parts);
        const target = path.join(this.root, ...parts);
        this.output = fs.openSync(
          target,
          fs.constants.O_RDWR |
            fs.constants.O_CREAT |
            fs.constants.O_EXCL |
            (fs.constants.O_NOFOLLOW || 0),
          header.mode,
        );
        this.header = header;
        this.remaining = header.size;
        this.treeHash.update(lengthBytes);
        this.treeHash.update(headerBytes);
        this.pending = this.pending.subarray(4 + length);
        if (this.remaining === 0) this.finishFile();
        continue;
      }
      const amount = Math.min(this.remaining, this.pending.length);
      const payload = this.pending.subarray(0, amount);
      writeAllAt(this.output, payload, this.header.size - this.remaining);
      this.treeHash.update(payload);
      this.remaining -= amount;
      this.pending = this.pending.subarray(amount);
      if (this.remaining === 0) this.finishFile();
    }
  }
  finishFile() {
    const descriptor = this.output;
    try {
      const state = fs.fstatSync(descriptor);
      if (
        !state.isFile() || state.size !== this.header.size || state.nlink !== 1 ||
        (state.mode & 0o7777) !== this.header.mode
      ) {
        throw new RunnerError("runtime archive file differs after write");
      }
      const digest = crypto.createHash("sha256");
      const buffer = Buffer.alloc(Math.min(1024 * 1024, this.header.size));
      let offset = 0;
      while (offset < this.header.size) {
        let amount;
        try {
          amount = fs.readSync(
            descriptor, buffer, 0, Math.min(buffer.length, this.header.size - offset), offset,
          );
        } catch (error) {
          if (error?.code === "EINTR") continue;
          throw error;
        }
        if (amount < 1) throw new RunnerError("runtime archive file ended after write");
        digest.update(buffer.subarray(0, amount));
        offset += amount;
      }
      const final = fs.fstatSync(descriptor);
      if (
        state.dev !== final.dev || state.ino !== final.ino || state.size !== final.size ||
        state.mode !== final.mode || state.nlink !== final.nlink ||
        digest.digest("hex") !== this.header.sha256
      ) {
        throw new RunnerError("runtime archive file digest differs after write");
      }
      this.inventory.push(Object.freeze({ mode: this.header.mode === 0o700 ? 0o500 : 0o400,
        path: this.header.path, sha256: this.header.sha256, size: this.header.size }));
      this.identities.set(this.header.path, Object.freeze({ dev: final.dev, ino: final.ino }));
    } finally {
      this.output = -1;
      fs.closeSync(descriptor);
    }
    this.header = null;
  }
  finish() {
    if (this.output >= 0) fs.closeSync(this.output);
    if (
      !this.finished ||
      this.pending.length ||
      this.header !== null ||
      this.fileCount !== this.manifest.fileCount ||
      this.unpackedBytes !== this.manifest.unpackedBytes ||
      this.treeHash.digest("hex") !== this.manifest.treeSha256
    ) {
      throw new RunnerError("runtime archive is incomplete or differs from its manifest");
    }
    return Object.freeze(this.inventory);
  }
}
class RuntimeTree {
  constructor(root, inventory, fileIdentities) {
    this.root = root;
    this.files = new Map(inventory.map((item) => [item.path, item]));
    this.fileIdentities = fileIdentities;
    this.directories = new Map();
    for (const item of inventory) {
      const parts = safeRelative(item.path);
      for (let length = 1; length < parts.length; length += 1) {
        this.directories.set(parts.slice(0, length).join("/"), null);
      }
    }
    this.descriptor = fs.openSync(
      root,
      fs.constants.O_RDONLY |
        (fs.constants.O_DIRECTORY || 0) |
        (fs.constants.O_NOFOLLOW || 0),
    );
    this.rootIdentity = fs.fstatSync(this.descriptor);
  }
  rootState(requireLocked) {
    const target = fs.lstatSync(this.root);
    const retained = fs.fstatSync(this.descriptor);
    if (
      !target.isDirectory() || target.isSymbolicLink() || !retained.isDirectory() ||
      !sameIdentity(target, retained) || !sameIdentity(retained, this.rootIdentity) ||
      (requireLocked && (target.mode & 0o7777) !== 0o500)
    ) throw new RunnerError("runtime root identity changed");
  }
  lock() {
    try {
      this.rootState(false);
      for (const [relative, item] of this.files) {
        const target = path.join(this.root, ...safeRelative(relative));
        const state = fs.lstatSync(target);
        if (!state.isFile() || state.isSymbolicLink() || !sameIdentity(state, this.fileIdentities.get(relative))) {
          throw new RunnerError("runtime file identity changed");
        }
        fs.chmodSync(target, item.mode);
      }
      const deepestFirst = [...this.directories].sort((left, right) => right[0].length - left[0].length);
      for (const [relative] of deepestFirst) {
        const target = path.join(this.root, ...safeRelative(relative));
        const state = fs.lstatSync(target);
        if (!state.isDirectory() || state.isSymbolicLink()) {
          throw new RunnerError("runtime directory identity changed");
        }
        this.directories.set(relative, Object.freeze({ dev: state.dev, ino: state.ino }));
        fs.chmodSync(target, 0o500);
      }
      fs.chmodSync(this.root, 0o500);
    } catch (error) {
      throw new RunnerError("Promptfoo runtime changed after attestation", { cause: error });
    }
  }
  verifyFile(target, relative, pathState) {
    const item = this.files.get(relative);
    const descriptor = fs.openSync(target, fs.constants.O_RDONLY | (fs.constants.O_NOFOLLOW || 0));
    try {
      const before = fs.fstatSync(descriptor);
      const identity = this.fileIdentities.get(relative);
      if (
        !item || !pathState.isFile() || pathState.isSymbolicLink() || !before.isFile() ||
        !sameIdentity(pathState, before) || !sameIdentity(before, identity) || before.nlink !== 1 ||
        before.size !== item.size || (before.mode & 0o7777) !== item.mode
      ) throw new RunnerError("runtime file differs");
      const digest = descriptorDigest(descriptor, item.size);
      const after = fs.fstatSync(descriptor);
      if (!sameIdentity(before, after) || before.size !== after.size || before.mode !== after.mode ||
          before.nlink !== after.nlink || digest !== item.sha256) {
        throw new RunnerError("runtime file changed while attested");
      }
    } finally {
      fs.closeSync(descriptor);
    }
  }
  scan(directory = this.root, prefix = "", foundFiles = new Set(), foundDirectories = new Set()) {
    for (const name of fs.readdirSync(directory).sort()) {
      const relative = prefix ? `${prefix}/${name}` : name;
      const target = path.join(directory, name);
      const state = fs.lstatSync(target);
      if (state.isDirectory() && !state.isSymbolicLink()) {
        const identity = this.directories.get(relative);
        if (!identity || !sameIdentity(state, identity) || (state.mode & 0o7777) !== 0o500) {
          throw new RunnerError("runtime directory differs");
        }
        foundDirectories.add(relative);
        this.scan(target, relative, foundFiles, foundDirectories);
      } else {
        if (!state.isFile() || state.isSymbolicLink() || !this.files.has(relative)) {
          throw new RunnerError("runtime path set differs");
        }
        this.verifyFile(target, relative, state);
        foundFiles.add(relative);
      }
    }
    if (prefix === "" &&
        (foundFiles.size !== this.files.size || foundDirectories.size !== this.directories.size)) {
      throw new RunnerError("runtime path set differs");
    }
  }
  attest() {
    try {
      this.rootState(true);
      this.scan();
    } catch (error) {
      throw new RunnerError("Promptfoo runtime changed after attestation", { cause: error });
    }
  }
  close() { fs.closeSync(this.descriptor); }
}
function runtimeManifest(manifestPath, chunkPaths) {
  const manifest = object(
    boundedJson(boundedFile(manifestPath, MAX_MANIFEST_BYTES, "runtime manifest"), "runtime manifest"),
    "runtime manifest",
  );
  const fields = (
    "archiveSha256 chunkSha256 entrypoint fileCount format maximumFileBytes " +
    "maximumUnpackedBytes nodeSha256 nodeVersion packageJsonSha256 platform " +
    "pnpmLockSha256 promptfooVersion schemaVersion treeSha256 unpackedBytes"
  ).split(" ");
  exactKeys(manifest, fields, "runtime manifest");
  if (
    manifest.schemaVersion !== 1 ||
    manifest.format !== "ai-ip-promptfoo-records-v1" ||
    manifest.promptfooVersion !== "0.122.0" ||
    manifest.entrypoint !== "node_modules/promptfoo/dist/src/entrypoint.js" ||
    manifest.nodeVersion !== process.version ||
    !nodeVersionSupported(manifest.nodeVersion) ||
    manifest.platform?.os !== process.platform ||
    manifest.platform?.arch !== ({ x64: "x86_64", arm64: "arm64" }[process.arch] || process.arch) ||
    !Array.isArray(manifest.chunkSha256) ||
    manifest.chunkSha256.length > MAX_CHUNKS ||
    manifest.chunkSha256.length !== chunkPaths.length ||
    !Number.isSafeInteger(manifest.fileCount) ||
    manifest.fileCount < 1 ||
    manifest.fileCount > MAX_FILES ||
    !Number.isSafeInteger(manifest.unpackedBytes) ||
    manifest.unpackedBytes < 1 ||
    !Number.isSafeInteger(manifest.maximumFileBytes) ||
    manifest.maximumFileBytes > 384 * 1024 * 1024 ||
    !Number.isSafeInteger(manifest.maximumUnpackedBytes) ||
    manifest.maximumUnpackedBytes > 2 * 1024 * 1024 * 1024 ||
    manifest.unpackedBytes > manifest.maximumUnpackedBytes
  ) {
    throw new RunnerError("runtime manifest is incompatible");
  }
  return manifest;
}

async function extractRuntime(manifestPath, chunkPaths, temporaryRoot) {
  const manifest = runtimeManifest(manifestPath, chunkPaths);
  const archiveHash = crypto.createHash("sha256");
  async function* chunks() {
    for (let index = 0; index < chunkPaths.length; index += 1) {
      const payload = boundedFile(chunkPaths[index], MAX_CHUNK_BYTES, "runtime chunk");
      if (sha256(payload) !== manifest.chunkSha256[index]) {
        throw new RunnerError("runtime chunk identity differs from manifest");
      }
      archiveHash.update(payload);
      yield payload;
    }
  }
  const root = fs.mkdtempSync(path.join(temporaryRoot, "promptfoo-runtime-"));
  fs.chmodSync(root, 0o700);
  const extractor = new RecordExtractor(root, manifest);
  const gunzip = createGunzip();
  Readable.from(chunks()).pipe(gunzip);
  for await (const chunk of gunzip) extractor.consume(chunk);
  const inventory = extractor.finish();
  if (archiveHash.digest("hex") !== manifest.archiveSha256) {
    throw new RunnerError("runtime archive identity differs from manifest");
  }
  const entrypoint = path.join(root, ...safeRelative(manifest.entrypoint));
  const state = fs.lstatSync(entrypoint);
  if (!state.isFile() || state.isSymbolicLink()) {
    throw new RunnerError("sealed Promptfoo entrypoint is unavailable");
  }
  let runtime;
  try {
    runtime = new RuntimeTree(root, inventory, extractor.identities);
    runtime.lock();
  } catch (error) {
    if (runtime) runtime.close();
    if (error instanceof RunnerError && error.message === "Promptfoo runtime changed after attestation") {
      throw error;
    }
    throw new RunnerError("Promptfoo runtime changed after attestation", { cause: error });
  }
  return { attest: () => runtime.attest(), close: () => runtime.close(), entrypoint };
}

async function main() {
  const [resultSupportPath, manifestPath, ...chunkPaths] = process.argv.slice(2);
  if (!resultSupportPath || !manifestPath || !chunkPaths.length) {
    throw new RunnerError("sealed runtime arguments are required");
  }
  const { canonical, parseResult } = require(resultSupportPath);
  const temporaryRoot = isolatedDirectory(process.env.TMPDIR, "TMPDIR");
  const promptfoo = isolatedDirectory(
    process.env.PROMPTFOO_CONFIG_DIR,
    "PROMPTFOO_CONFIG_DIR",
  );
  const outputPath = isolatedOutput(process.env.PROMPTFOO_OUTPUT_PATH, promptfoo);
  const runtime = await extractRuntime(manifestPath, chunkPaths, temporaryRoot);
  let completed;
  try {
    const configPath = path.join(promptfoo, "eval-config.json");
    fs.writeFileSync(
      configPath,
      boundedFile(process.env.AI_IP_PROMPTFOO_PATH, MAX_CONFIG_BYTES, "Promptfoo config"),
      { flag: "wx", mode: 0o600 },
    );
    const command = ["eval", "--config", configPath, "--output", outputPath];
    command.push(
      "--no-cache", "--no-progress-bar", "--no-table", "--no-share", "--no-write",
      "--max-concurrency", "1",
    );
    const candidateTmp = fs.mkdtempSync(path.join(process.cwd(), ".ai-ip-candidate-tmp-"));
    fs.chmodSync(candidateTmp, 0o700);
    const environment = promptfooEnvironment();
    environment.AI_IP_CANDIDATE_TMP = candidateTmp;
    runtime.attest();
    completed = spawnSync(process.execPath, [runtime.entrypoint, ...command], {
      cwd: process.cwd(), env: environment, stdio: ["ignore", "inherit", "inherit"],
    });
    runtime.attest();
  } finally {
    runtime.close();
  }
  if (completed.error) throw completed.error;
  if (completed.status !== 0) return completed.status ?? 2;
  const parsed = parseResult(
    boundedFile(outputPath, MAX_RESULT_BYTES, "Promptfoo result"),
  );
  writeAll(Number(process.env.AI_IP_RESULT_FD), Buffer.from(canonical(parsed.result)));
  writeAll(Number(process.env.AI_IP_TELEMETRY_FD), Buffer.from(canonical(parsed.telemetry)));
  return 0;
}

module.exports = { nodeVersionSupported, safeRelative, writeAll, writeAllAt };

if (require.main === module) {
  main()
    .then((status) => {
      process.exitCode = status;
    })
    .catch((error) => {
      const message = `Promptfoo sealed runner failed: ${error.name}: ${error.message}\n`;
      fs.writeSync(2, Buffer.from(message).subarray(0, 4096));
      process.exitCode = 2;
    });
}
