"use strict";
const crypto = require("node:crypto"), fs = require("node:fs"), path = require("node:path");
const { spawnSync } = require("node:child_process"), { Readable } = require("node:stream"), { createGunzip } = require("node:zlib");
const supportPath = require.main === module && process.argv[2] ? process.argv[2] : path.join(__dirname, "batch_promptfoo_result.js");
const { RunnerError, boundedFile, canonical, closeAll, descriptorDigest, ensureParents, isolatedDirectory, isolatedOutput, nodeVersionSupported, safeRelative, sameIdentity, sha256, writeAll, writeAllAt } = require(supportPath);
const MAX_CHUNK_BYTES = 16 * 1024 * 1024, MAX_MANIFEST_BYTES = 1024 * 1024;
const MAX_CONFIG_BYTES = 1024 * 1024, MAX_RESULT_BYTES = 8 * 1024 * 1024, MAX_HEADER_BYTES = 4096, MAX_CHUNKS = 29, MAX_FILES = 100000;
function boundedJson(payload, label) {
  let depth = 0, nodes = 1, inString = false, escaped = false;
  for (const byte of payload) {
    if (inString) {
      if (escaped) escaped = false; else if (byte === 0x5c) escaped = true;
      else if (byte === 0x22) inString = false;
      continue;
    }
    if (byte === 0x22) inString = true;
    else if (byte === 0x5b || byte === 0x7b) {
      depth += 1; nodes += 1;
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
  try { return JSON.parse(payload.toString("utf8")); }
  catch (error) { throw new RunnerError(`${label} is not valid JSON`, { cause: error }); }
}
function object(value, label) {
  if (value === null || typeof value !== "object" || Array.isArray(value))
    throw new RunnerError(`${label} must be an object`);
  return value;
}
function exactKeys(value, keys, label) {
  if (Object.keys(value).sort().join("\0") !== [...keys].sort().join("\0"))
    throw new RunnerError(`${label} has invalid fields`);
}
function frame(hash, payload) {
  const length = Buffer.alloc(4); length.writeUInt32BE(payload.length); hash.update(length).update(payload);
}
function inventorySha256(records) {
  if (!Array.isArray(records)) throw new RunnerError("runtime inventory is invalid");
  const hash = crypto.createHash("sha256").update(Buffer.from("ai-ip-promptfoo-inventory/v1\0"));
  let previous = null;
  for (const item of records) {
    const record = object(item, "runtime inventory record");
    exactKeys(record, ["mode", "path", "sha256", "size"], "runtime inventory record"); safeRelative(record.path);
    const encoded = Buffer.from(record.path, "utf8");
    if ((previous && Buffer.compare(previous, encoded) >= 0) ||
        ![0o400, 0o500].includes(record.mode) || !Number.isSafeInteger(record.size) ||
        record.size < 0 || record.size > 384 * 1024 * 1024 || !/^[0-9a-f]{64}$/.test(record.sha256))
      throw new RunnerError("runtime inventory record is invalid");
    hash.update(Buffer.from("file\0"));
    for (const value of [encoded, Buffer.from(String(record.mode)),
      Buffer.from(record.sha256, "hex"), Buffer.from(String(record.size))]) frame(hash, value);
    previous = encoded;
  }
  return hash.update(Buffer.from("end\0")).digest("hex");
}
function objectIdentitySha256(records) {
  const hash = crypto.createHash("sha256").update(Buffer.from("ai-ip-promptfoo-object-identity/v1\0"));
  let previous = null;
  for (const record of records) {
    const encoded = Buffer.from(record.path, "utf8");
    if ((record.path && safeRelative(record.path).length < 1) ||
        (!record.path && record.kind !== "root") || (previous && Buffer.compare(previous, encoded) >= 0) ||
        !["root", "directory", "file"].includes(record.kind))
      throw new RunnerError("runtime object identity is invalid");
    hash.update(Buffer.from(`${record.kind}\0`)); frame(hash, encoded);
    for (const name of ["dev", "ino", "mode", "nlink", "size", "mtimeNs", "ctimeNs"]) {
      if (typeof record.state[name] !== "bigint") throw new RunnerError("runtime object identity is invalid");
      frame(hash, Buffer.from(String(record.state[name])));
    }
    previous = encoded;
  }
  return hash.update(Buffer.from("end\0")).digest("hex");
}
function emitAttestation(fd, value, writer = fs.writeSync, closer = fs.closeSync) {
  let failure = null;
  try {
    const payload = Buffer.from(`${canonical(value)}\n`);
    if (payload.length > 8192) throw new RunnerError("runtime attestation envelope exceeds its byte bound");
    writeAll(fd, payload, writer);
  } catch (error) { failure = error; }
  try { closer(fd); } catch (error) { failure ||= error; }
  if (failure) throw failure;
}
function privateAttestationInput() {
  const raw = process.env.AI_IP_RUNTIME_ATTESTATION_FD, attemptId = process.env.AI_IP_ATTEMPT_ID;
  delete process.env.AI_IP_RUNTIME_ATTESTATION_FD; delete process.env.AI_IP_ATTEMPT_ID;
  if (typeof raw !== "string" || !/^[1-9]\d*$/.test(raw))
    throw new RunnerError("runtime attestation FD is invalid");
  const fd = Number(raw);
  if (!Number.isSafeInteger(fd) || fd < 3 || fd > 0x7fffffff)
    throw new RunnerError("runtime attestation FD is invalid");
  let failure = null;
  if (!/^[0-9a-f]{64}$/.test(attemptId || "")) failure = new RunnerError("attempt ID is invalid");
  for (const name of ["AI_IP_RESULT_FD", "AI_IP_TELEMETRY_FD"])
    if (/^(0|[1-9]\d*)$/.test(process.env[name] || "") && Number(process.env[name]) === fd)
      failure ||= new RunnerError("runtime attestation FD is not private");
  try { fs.fstatSync(fd); }
  catch (error) { failure ||= new RunnerError("runtime attestation FD is unavailable", { cause: error }); }
  if (failure) closeAll([fd], failure);
  return { attemptId, fd };
}
function promptfooEnvironment() {
  const names = (
    "AI_IP_CASE_PATH AI_IP_CODEX_PATH AI_IP_CONFIG_PATH AI_IP_PROFILE_PATH AI_IP_PROMPTFOO_PATH AI_IP_PROTOCOL_PATH " +
    "AI_IP_RESULT_FD AI_IP_ROUTE_PATH AI_IP_SCHEMA_PATH AI_IP_TELEMETRY_FD APPDATA AWS_PROFILE CODEX_HOME COMSPEC FORCE_COLOR HOME " +
    "HOMEDRIVE HOMEPATH LANG LANGUAGE LC_ALL LC_COLLATE LC_CTYPE LC_MESSAGES LC_MONETARY LC_NUMERIC LC_TIME LOCALAPPDATA NO_PROXY " +
    "PATH PATHEXT PROMPTFOO_CACHE_ENABLED PROMPTFOO_CACHE_PATH PROMPTFOO_CONFIG_DIR PROMPTFOO_DISABLE_SHARING PROMPTFOO_DISABLE_TELEMETRY " +
    "PROMPTFOO_DISABLE_UPDATE PROMPTFOO_OUTPUT_PATH SHELL SYSTEMROOT TEMP TMP TMPDIR USERPROFILE VOLCENGINE_PROFILE").split(" ");
  for (const name of ["PROMPTFOO_DISABLE_SHARING", "PROMPTFOO_DISABLE_TELEMETRY", "PROMPTFOO_DISABLE_UPDATE"]) {
    if (process.env[name] !== "1") throw new RunnerError(`${name} must be sealed off`);
  }
  const environment = Object.fromEntries(names.filter((name) => process.env[name] !== undefined)
    .map((name) => [name, process.env[name]]));
  environment.AI_IP_WORKSPACE = process.cwd();
  return environment;
}
class RecordExtractor {
  constructor(root, manifest) {
    this.root = root; this.manifest = manifest; this.pending = Buffer.alloc(0);
    this.header = null; this.output = -1; this.remaining = 0;
    this.treeHash = crypto.createHash("sha256");
    this.fileCount = 0; this.unpackedBytes = 0; this.inflatedBytes = 0;
    this.maximumInflatedBytes = manifest.unpackedBytes + manifest.fileCount * (MAX_HEADER_BYTES + 4) + 4;
    this.finished = false; this.inventory = [];
    this.identities = new Map();
  }
  consume(chunk) {
    this.pending = this.pending.length ? Buffer.concat([this.pending, chunk]) : chunk;
    if (this.header === null && this.pending.length >= 4) {
      const declaredHeader = this.pending.readUInt32BE(0);
      if (declaredHeader > MAX_HEADER_BYTES) throw new RunnerError("runtime header exceeds its byte bound");
    }
    this.inflatedBytes += chunk.length;
    if (this.inflatedBytes > this.maximumInflatedBytes)
      throw new RunnerError("runtime inflated stream exceeds its byte bound");
    while (this.pending.length) {
      if (this.finished) throw new RunnerError("runtime archive has trailing bytes");
      if (this.header === null) {
        if (this.pending.length < 4) return;
        const lengthBytes = this.pending.subarray(0, 4);
        const length = lengthBytes.readUInt32BE(0);
        if (length === 0) {
          this.treeHash.update(lengthBytes); this.pending = this.pending.subarray(4); this.finished = true;
          continue;
        }
        if (length > MAX_HEADER_BYTES) throw new RunnerError("runtime header exceeds its byte bound");
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
        this.fileCount += 1; this.unpackedBytes += header.size;
        if (
          this.fileCount > this.manifest.fileCount ||
          this.unpackedBytes > this.manifest.maximumUnpackedBytes
        ) {
          throw new RunnerError("runtime archive expanded content exceeds its bound");
        }
        ensureParents(this.root, parts);
        const target = path.join(this.root, ...parts);
        this.output = fs.openSync(target, fs.constants.O_RDWR | fs.constants.O_CREAT |
          fs.constants.O_EXCL | (fs.constants.O_NOFOLLOW || 0), header.mode);
        this.header = header; this.remaining = header.size;
        this.treeHash.update(lengthBytes); this.treeHash.update(headerBytes);
        this.pending = this.pending.subarray(4 + length);
        if (this.remaining === 0) this.finishFile();
        continue;
      }
      const amount = Math.min(this.remaining, this.pending.length);
      const payload = this.pending.subarray(0, amount);
      writeAllAt(this.output, payload, this.header.size - this.remaining);
      this.treeHash.update(payload); this.remaining -= amount;
      this.pending = this.pending.subarray(amount);
      if (this.remaining === 0) this.finishFile();
    }
  }
  finishFile() {
    const descriptor = this.output;
    try {
      const state = fs.fstatSync(descriptor, { bigint: true });
      if (!state.isFile() || state.size !== BigInt(this.header.size) || state.nlink !== 1n ||
          (state.mode & 0o7777n) !== BigInt(this.header.mode)) {
        throw new RunnerError("runtime archive file differs after write");
      }
      const digest = crypto.createHash("sha256");
      const buffer = Buffer.alloc(Math.min(1024 * 1024, this.header.size));
      let offset = 0;
      while (offset < this.header.size) {
        let amount;
        try { amount = fs.readSync(descriptor, buffer, 0,
          Math.min(buffer.length, this.header.size - offset), offset); } catch (error) {
          if (error?.code === "EINTR") continue;
          throw error;
        }
        if (amount < 1) throw new RunnerError("runtime archive file ended after write");
        digest.update(buffer.subarray(0, amount)); offset += amount;
      }
      const final = fs.fstatSync(descriptor, { bigint: true });
      if (state.dev !== final.dev || state.ino !== final.ino || state.size !== final.size ||
        state.mode !== final.mode || state.nlink !== final.nlink ||
        digest.digest("hex") !== this.header.sha256) {
        throw new RunnerError("runtime archive file digest differs after write");
      }
      this.inventory.push(Object.freeze({ mode: this.header.mode === 0o700 ? 0o500 : 0o400,
        path: this.header.path, sha256: this.header.sha256, size: this.header.size }));
      this.identities.set(this.header.path, Object.freeze({ dev: final.dev, ino: final.ino }));
    } finally { this.output = -1; fs.closeSync(descriptor); }
    this.header = null;
  }
  finish() {
    if (this.output >= 0) fs.closeSync(this.output);
    if (!this.finished || this.pending.length || this.header !== null ||
      this.fileCount !== this.manifest.fileCount ||
      this.unpackedBytes !== this.manifest.unpackedBytes ||
      this.treeHash.digest("hex") !== this.manifest.treeSha256) {
      throw new RunnerError("runtime archive is incomplete or differs from its manifest");
    }
    const ordered = [...this.inventory].sort((left, right) =>
      Buffer.compare(Buffer.from(left.path, "utf8"), Buffer.from(right.path, "utf8")));
    if (inventorySha256(ordered) !== this.manifest.inventorySha256)
      throw new RunnerError("runtime inventory differs from its manifest");
    return Object.freeze(this.inventory);
  }
}
class RuntimeTree {
  constructor(root, inventory, fileIdentities) {
    this.root = root; this.files = new Map(inventory.map((item) => [item.path, item]));
    this.fileIdentities = fileIdentities;
    this.directories = new Map();
    for (const item of inventory) {
      const parts = safeRelative(item.path);
      for (let length = 1; length < parts.length; length += 1)
        this.directories.set(parts.slice(0, length).join("/"), null);
    }
    const descriptor = fs.openSync(root, fs.constants.O_RDONLY |
      (fs.constants.O_DIRECTORY || 0) | (fs.constants.O_NOFOLLOW || 0));
    try { this.rootIdentity = fs.fstatSync(descriptor, { bigint: true }); }
    catch (error) { closeAll([descriptor], error); }
    this.descriptor = descriptor;
  }
  rootState(requireLocked) {
    const target = fs.lstatSync(this.root, { bigint: true });
    const retained = fs.fstatSync(this.descriptor, { bigint: true });
    if (
      !target.isDirectory() || target.isSymbolicLink() || !retained.isDirectory() ||
      !sameIdentity(target, retained) || !sameIdentity(retained, this.rootIdentity) ||
      (requireLocked && (target.mode & 0o7777n) !== 0o500n)
    ) throw new RunnerError("runtime root identity changed");
  }
  lock() {
    try {
      this.rootState(false);
      for (const [relative, item] of this.files) {
        const target = path.join(this.root, ...safeRelative(relative));
        const state = fs.lstatSync(target, { bigint: true });
        if (!state.isFile() || state.isSymbolicLink() || !sameIdentity(state, this.fileIdentities.get(relative)))
          throw new RunnerError("runtime file identity changed");
        fs.chmodSync(target, item.mode);
      }
      const deepestFirst = [...this.directories].sort((left, right) => right[0].length - left[0].length);
      for (const [relative] of deepestFirst) {
        const target = path.join(this.root, ...safeRelative(relative));
        const state = fs.lstatSync(target, { bigint: true });
        if (!state.isDirectory() || state.isSymbolicLink()) throw new RunnerError("runtime directory identity changed");
        this.directories.set(relative, Object.freeze({ dev: state.dev, ino: state.ino }));
        fs.chmodSync(target, 0o500);
      }
      fs.chmodSync(this.root, 0o500);
    } catch (error) { throw new RunnerError("Promptfoo runtime changed after attestation", { cause: error }); }
  }
  inspectDirectory(capability) {
    const target = fs.lstatSync(capability.target, { bigint: true }), retained = fs.fstatSync(capability.descriptor, { bigint: true });
    const identity = capability.relative ? this.directories.get(capability.relative) :
      this.rootIdentity;
    if (!identity || !target.isDirectory() || target.isSymbolicLink() || !retained.isDirectory() ||
        !sameIdentity(target, retained) || !sameIdentity(retained, identity) ||
        (target.mode & 0o7777n) !== 0o500n || (retained.mode & 0o7777n) !== 0o500n)
      throw new RunnerError("runtime directory differs");
    return retained;
  }
  directoryState(capability) {
    const before = this.inspectDirectory(capability), names = fs.readdirSync(capability.target).sort();
    const after = this.inspectDirectory(capability);
    if (!sameIdentity(before, after) || before.mode !== after.mode || before.nlink !== after.nlink || before.size !== after.size ||
        before.mtimeNs !== after.mtimeNs || before.ctimeNs !== after.ctimeNs) throw new RunnerError("runtime directory changed while attested");
    return { names, state: after };
  }
  fileState(capability) {
    const item = this.files.get(capability.relative);
    const target = fs.lstatSync(capability.target, { bigint: true });
    const before = fs.fstatSync(capability.descriptor, { bigint: true });
    const identity = this.fileIdentities.get(capability.relative);
    if (!item || !target.isFile() || target.isSymbolicLink() || !before.isFile() ||
        !sameIdentity(target, before) || !sameIdentity(before, identity) || before.nlink !== 1n ||
        before.size !== BigInt(item.size) || (before.mode & 0o7777n) !== BigInt(item.mode))
      throw new RunnerError("runtime file differs");
    const digest = descriptorDigest(capability.descriptor, item.size);
    const after = fs.fstatSync(capability.descriptor, { bigint: true });
    if (!sameIdentity(before, after) || before.size !== after.size || before.mode !== after.mode ||
        before.nlink !== after.nlink || before.mtimeNs !== after.mtimeNs ||
        before.ctimeNs !== after.ctimeNs || digest !== item.sha256) {
      throw new RunnerError("runtime file changed while attested");
    }
    return after;
  }
  scan(capability, capabilities, foundFiles, foundDirectories) {
    capability.names = this.directoryState(capability).names;
    for (const name of capability.names) {
      const relative = capability.relative ? `${capability.relative}/${name}` : name;
      const target = path.join(capability.target, name);
      const state = fs.lstatSync(target, { bigint: true });
      if (state.isDirectory() && !state.isSymbolicLink()) {
        if (!this.directories.has(relative)) throw new RunnerError("runtime path set differs");
        const descriptor = fs.openSync(target, fs.constants.O_RDONLY |
          (fs.constants.O_DIRECTORY || 0) | (fs.constants.O_NOFOLLOW || 0));
        const child = { descriptor, relative, target };
        capabilities.push(child); foundDirectories.add(relative);
        this.scan(child, capabilities, foundFiles, foundDirectories);
      } else {
        if (!state.isFile() || state.isSymbolicLink() || !this.files.has(relative))
          throw new RunnerError("runtime path set differs");
        const descriptor = fs.openSync(target, fs.constants.O_RDONLY |
          (fs.constants.O_NOFOLLOW || 0));
        const child = { descriptor, relative, target, file: true };
        capabilities.push(child); this.fileState(child); foundFiles.add(relative);
      }
    }
  }
  attest() {
    const root = { descriptor: this.descriptor, names: [], relative: "", target: this.root };
    const capabilities = [root];
    let failure = null, records;
    try {
      const foundFiles = new Set(), foundDirectories = new Set();
      this.scan(root, capabilities, foundFiles, foundDirectories);
      if (foundFiles.size !== this.files.size || foundDirectories.size !== this.directories.size)
        throw new RunnerError("runtime path set differs");
      records = [...capabilities].reverse().map((capability) => {
        let state;
        if (capability.file) state = this.fileState(capability);
        else {
          const observed = this.directoryState(capability);
          if (observed.names.join("\0") !== capability.names.join("\0"))
            throw new RunnerError("runtime path set differs");
          state = observed.state;
        }
        return { kind: capability.relative ? (capability.file ? "file" : "directory") : "root",
          path: capability.relative, state };
      });
      records.sort((left, right) => Buffer.compare(Buffer.from(left.path), Buffer.from(right.path)));
    } catch (error) { failure = new RunnerError("Promptfoo runtime changed after attestation", { cause: error }); }
    closeAll(capabilities.slice(1).map((capability) => capability.descriptor), failure);
    const rootState = records.find((record) => record.path === "").state;
    return { objectIdentitySha256: objectIdentitySha256(records),
      rootDevice: String(rootState.dev), rootInode: String(rootState.ino) };
  }
  close(primary = null) { closeAll([this.descriptor], primary); }
}
function runtimeManifest(manifestPath, chunkPaths) {
  const payload = boundedFile(manifestPath, MAX_MANIFEST_BYTES, "runtime manifest");
  const manifest = object(boundedJson(payload, "runtime manifest"), "runtime manifest");
  const fields = (
    "archiveSha256 chunkSha256 entrypoint fileCount format maximumFileBytes " +
    "inventorySha256 maximumUnpackedBytes nodeSha256 nodeVersion packageJsonSha256 platform " +
    "pnpmLockSha256 promptfooVersion schemaVersion treeSha256 unpackedBytes"
  ).split(" ");
  exactKeys(manifest, fields, "runtime manifest");
  if (manifest.schemaVersion !== 1 || manifest.format !== "ai-ip-promptfoo-records-v1" ||
    manifest.promptfooVersion !== "0.122.0" || manifest.entrypoint !== "node_modules/promptfoo/dist/src/entrypoint.js" ||
    manifest.nodeVersion !== process.version || !nodeVersionSupported(manifest.nodeVersion) ||
    manifest.platform?.os !== process.platform ||
    manifest.platform?.arch !== ({ x64: "x86_64", arm64: "arm64" }[process.arch] || process.arch) ||
    !Array.isArray(manifest.chunkSha256) || manifest.chunkSha256.length > MAX_CHUNKS ||
    manifest.chunkSha256.length !== chunkPaths.length ||
    !Number.isSafeInteger(manifest.fileCount) || manifest.fileCount < 1 || manifest.fileCount > MAX_FILES ||
    !Number.isSafeInteger(manifest.unpackedBytes) || manifest.unpackedBytes < 1 ||
    !Number.isSafeInteger(manifest.maximumFileBytes) ||
    manifest.maximumFileBytes > 384 * 1024 * 1024 ||
    !Number.isSafeInteger(manifest.maximumUnpackedBytes) ||
    manifest.maximumUnpackedBytes > 2 * 1024 * 1024 * 1024 ||
    manifest.unpackedBytes > manifest.maximumUnpackedBytes || !/^[0-9a-f]{64}$/.test(manifest.inventorySha256))
    throw new RunnerError("runtime manifest is incompatible");
  return { manifest, manifestSha256: sha256(payload) };
}
async function extractRuntime(manifestPath, chunkPaths, temporaryRoot) {
  const sealed = runtimeManifest(manifestPath, chunkPaths), manifest = sealed.manifest;
  const archiveHash = crypto.createHash("sha256");
  async function* chunks() {
    for (let index = 0; index < chunkPaths.length; index += 1) {
      const payload = boundedFile(chunkPaths[index], MAX_CHUNK_BYTES, "runtime chunk");
      if (sha256(payload) !== manifest.chunkSha256[index])
        throw new RunnerError("runtime chunk identity differs from manifest");
      archiveHash.update(payload);
      yield payload;
    }
  }
  const root = path.join(temporaryRoot, "promptfoo-runtime");
  try { fs.mkdirSync(root, { mode: 0o700 }); } catch (error) {
    throw new RunnerError("runtime root already exists or is unavailable", { cause: error }); }
  fs.chmodSync(root, 0o700);
  const extractor = new RecordExtractor(root, manifest), gunzip = createGunzip();
  Readable.from(chunks()).pipe(gunzip);
  for await (const chunk of gunzip) extractor.consume(chunk);
  const inventory = extractor.finish();
  if (archiveHash.digest("hex") !== manifest.archiveSha256)
    throw new RunnerError("runtime archive identity differs from manifest");
  const entrypoint = path.join(root, ...safeRelative(manifest.entrypoint));
  const state = fs.lstatSync(entrypoint);
  if (!state.isFile() || state.isSymbolicLink()) throw new RunnerError("sealed Promptfoo entrypoint is unavailable");
  let runtime;
  try {
    runtime = new RuntimeTree(root, inventory, extractor.identities);
    runtime.lock();
  } catch (error) {
    const failure = error instanceof RunnerError &&
      error.message === "Promptfoo runtime changed after attestation" ? error :
      new RunnerError("Promptfoo runtime changed after attestation", { cause: error });
    if (runtime) runtime.close(failure);
    throw failure;
  }
  return { attest: () => runtime.attest(), close: (error) => runtime.close(error), entrypoint,
    fileCount: manifest.fileCount, inventorySha256: manifest.inventorySha256,
    manifestSha256: sealed.manifestSha256, unpackedBytes: manifest.unpackedBytes };
}
async function main() {
  const privateInput = privateAttestationInput();
  try {
  const [resultSupportPath, manifestPath, ...chunkPaths] = process.argv.slice(2);
  if (!resultSupportPath || !manifestPath || !chunkPaths.length)
    throw new RunnerError("sealed runtime arguments are required");
  const { parseResult } = require(resultSupportPath);
  const temporaryRoot = isolatedDirectory(process.env.TMPDIR, "TMPDIR");
  const promptfoo = isolatedDirectory(process.env.PROMPTFOO_CONFIG_DIR, "PROMPTFOO_CONFIG_DIR");
  const outputPath = isolatedOutput(process.env.PROMPTFOO_OUTPUT_PATH, promptfoo);
  const runtime = await extractRuntime(manifestPath, chunkPaths, temporaryRoot);
  let completed, failure = null;
  try {
    const configPath = path.join(promptfoo, "eval-config.json");
    fs.writeFileSync(configPath, boundedFile(process.env.AI_IP_PROMPTFOO_PATH, MAX_CONFIG_BYTES,
      "Promptfoo config"), { flag: "wx", mode: 0o600 });
    const command = ["eval", "--config", configPath, "--output", outputPath];
    command.push("--no-cache", "--no-progress-bar", "--no-table", "--no-share", "--no-write",
      "--max-concurrency", "1");
    const candidateTmp = fs.mkdtempSync(path.join(process.cwd(), ".ai-ip-candidate-tmp-"));
    fs.chmodSync(candidateTmp, 0o700); const environment = promptfooEnvironment();
    environment.AI_IP_CANDIDATE_TMP = candidateTmp;
    const identity = runtime.attest();
    try {
      emitAttestation(privateInput.fd, {
        attemptId: privateInput.attemptId, fileCount: runtime.fileCount,
        inventorySha256: runtime.inventorySha256, manifestSha256: runtime.manifestSha256,
        objectIdentitySha256: identity.objectIdentitySha256,
        rootDevice: identity.rootDevice, rootInode: identity.rootInode,
        schemaVersion: "ai-ip-promptfoo-runtime-attestation/v1",
        unpackedBytes: runtime.unpackedBytes,
      });
    } finally { privateInput.fd = -1; }
    completed = spawnSync(process.execPath, [runtime.entrypoint, ...command], {
      cwd: process.cwd(), env: environment, stdio: ["ignore", "inherit", "inherit"],
    });
    runtime.attest();
  } catch (error) { failure = error; }
  runtime.close(failure);
  if (completed.error) throw completed.error;
  if (completed.status !== 0) return completed.status ?? 2;
  const parsed = parseResult(boundedFile(outputPath, MAX_RESULT_BYTES, "Promptfoo result"));
  writeAll(Number(process.env.AI_IP_RESULT_FD), Buffer.from(canonical(parsed.result)));
  writeAll(Number(process.env.AI_IP_TELEMETRY_FD), Buffer.from(canonical(parsed.telemetry)));
  return 0;
  } catch (error) {
    if (privateInput.fd >= 0) closeAll([privateInput.fd], error);
    throw error;
  }
}
module.exports = { nodeVersionSupported, safeRelative, writeAll, writeAllAt };
if (require.main === module) {
  main().then((status) => { process.exitCode = status; }).catch((error) => {
      const message = `Promptfoo sealed runner failed: ${error.name}: ${error.message}\n`;
      fs.writeSync(2, Buffer.from(message).subarray(0, 4096));
      process.exitCode = 2;
    });
}
