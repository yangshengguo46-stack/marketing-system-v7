"use strict";

const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
const { spawnSync } = require("node:child_process");
const { Readable } = require("node:stream");
const { createGunzip } = require("node:zlib");

const MAX_CHUNK_BYTES = 16 * 1024 * 1024;
const MAX_MANIFEST_BYTES = 1024 * 1024;
const MAX_CONFIG_BYTES = 1024 * 1024;
const MAX_RESULT_BYTES = 8 * 1024 * 1024;
const MAX_HEADER_BYTES = 4096;
const MAX_CHUNKS = 29;
const MAX_FILES = 100000;
const MAX_EVENTS = 1024;
const MAX_JSON_DEPTH = 64;
const MAX_JSON_NODES = 10000;
class RunnerError extends Error {}

function nodeVersionSupported(value) {
  const matched = /^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.exec(value);
  if (!matched) return false;
  const version = matched.slice(1).map(Number);
  return (
    version[0] > 22 ||
    (version[0] === 22 && (version[1] > 22 || (version[1] === 22 && version[2] >= 0)))
  );
}

function sha256(payload) {
  return crypto.createHash("sha256").update(payload).digest("hex");
}

function boundedFile(filePath, maximum, label) {
  const descriptor = fs.openSync(
    filePath,
    fs.constants.O_RDONLY | (fs.constants.O_NOFOLLOW || 0),
  );
  try {
    const before = fs.fstatSync(descriptor);
    if (!before.isFile() || before.size > maximum) {
      throw new RunnerError(`${label} is not a bounded regular file`);
    }
    const payload = Buffer.alloc(before.size);
    let offset = 0;
    while (offset < payload.length) {
      const amount = fs.readSync(
        descriptor,
        payload,
        offset,
        payload.length - offset,
        offset,
      );
      if (amount < 1) {
        throw new RunnerError(`${label} ended before its declared size`);
      }
      offset += amount;
    }
    const after = fs.fstatSync(descriptor);
    if (before.dev !== after.dev || before.ino !== after.ino || before.size !== after.size) {
      throw new RunnerError(`${label} changed while read`);
    }
    return payload;
  } finally {
    fs.closeSync(descriptor);
  }
}

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
      if (depth > MAX_JSON_DEPTH) throw new RunnerError(`${label} exceeds JSON depth`);
    } else if (byte === 0x5d || byte === 0x7d) {
      depth -= 1;
      if (depth < 0) throw new RunnerError(`${label} has invalid JSON shape`);
    } else if (byte === 0x2c || byte === 0x3a) {
      nodes += 1;
      if (nodes > MAX_JSON_NODES) throw new RunnerError(`${label} exceeds JSON nodes`);
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

function safeRelative(value) {
  const encoded = typeof value === "string" ? Buffer.from(value, "utf8") : null;
  if (
    typeof value !== "string" ||
    value.length < 1 ||
    encoded.toString("utf8") !== value ||
    encoded.length > 4096 ||
    value.includes("\\") ||
    /[\x00-\x1f\x7f]/.test(value) ||
    path.posix.isAbsolute(value)
  ) {
    throw new RunnerError("runtime archive path is unsafe");
  }
  const parts = value.split("/");
  if (
    parts.length > 128 ||
    parts.some((part) => part === "" || part === "." || part === "..")
  ) {
    throw new RunnerError("runtime archive path traversal is prohibited");
  }
  return parts;
}

function writeAll(fd, payload, writer = fs.writeSync) {
  let offset = 0;
  while (offset < payload.length) {
    let amount;
    try {
      amount = writer(fd, payload, offset, payload.length - offset);
    } catch (error) {
      if (error?.code === "EINTR") continue;
      throw error;
    }
    if (!Number.isSafeInteger(amount) || amount < 1 || amount > payload.length - offset) {
      throw new RunnerError("pipe write made no progress");
    }
    offset += amount;
  }
}

function isolatedDirectory(value, label) {
  if (typeof value !== "string" || !path.isAbsolute(value)) {
    throw new RunnerError(`${label} must be an absolute isolated directory`);
  }
  const resolved = path.resolve(value);
  const state = fs.lstatSync(resolved);
  if (!state.isDirectory() || state.isSymbolicLink() || fs.realpathSync(resolved) !== resolved) {
    throw new RunnerError(`${label} is not a real isolated directory`);
  }
  return resolved;
}

function isolatedOutput(value, parent) {
  if (typeof value !== "string" || !path.isAbsolute(value)) {
    throw new RunnerError("Promptfoo output must be an absolute isolated path");
  }
  const resolved = path.resolve(value);
  if (path.dirname(resolved) !== parent || path.basename(resolved) !== "output.json") {
    throw new RunnerError("Promptfoo output escaped its isolated directory");
  }
  if (fs.existsSync(resolved)) throw new RunnerError("Promptfoo output already exists");
  return resolved;
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

function ensureParents(root, parts) {
  let current = root;
  for (const part of parts.slice(0, -1)) {
    current = path.join(current, part);
    try {
      fs.mkdirSync(current, { mode: 0o700 });
    } catch (error) {
      if (error.code !== "EEXIST") throw error;
      const state = fs.lstatSync(current);
      if (!state.isDirectory() || state.isSymbolicLink()) {
        throw new RunnerError("runtime archive parent is unsafe");
      }
    }
  }
}

class RecordExtractor {
  constructor(root, manifest) {
    this.root = root;
    this.manifest = manifest;
    this.pending = Buffer.alloc(0);
    this.header = null;
    this.output = -1;
    this.remaining = 0;
    this.fileHash = null;
    this.treeHash = crypto.createHash("sha256");
    this.fileCount = 0;
    this.unpackedBytes = 0;
    this.inflatedBytes = 0;
    this.maximumInflatedBytes =
      manifest.unpackedBytes + manifest.fileCount * (MAX_HEADER_BYTES + 4) + 4;
    this.finished = false;
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
          fs.constants.O_WRONLY |
            fs.constants.O_CREAT |
            fs.constants.O_EXCL |
            (fs.constants.O_NOFOLLOW || 0),
          header.mode,
        );
        this.header = header;
        this.remaining = header.size;
        this.fileHash = crypto.createHash("sha256");
        this.treeHash.update(lengthBytes);
        this.treeHash.update(headerBytes);
        this.pending = this.pending.subarray(4 + length);
        if (this.remaining === 0) this.finishFile();
        continue;
      }
      const amount = Math.min(this.remaining, this.pending.length);
      const payload = this.pending.subarray(0, amount);
      fs.writeSync(this.output, payload);
      this.fileHash.update(payload);
      this.treeHash.update(payload);
      this.remaining -= amount;
      this.pending = this.pending.subarray(amount);
      if (this.remaining === 0) this.finishFile();
    }
  }

  finishFile() {
    fs.closeSync(this.output);
    this.output = -1;
    if (this.fileHash.digest("hex") !== this.header.sha256) {
      throw new RunnerError("runtime archive file digest differs");
    }
    this.header = null;
    this.fileHash = null;
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
  }
}

function runtimeManifest(manifestPath, chunkPaths) {
  const manifest = object(
    boundedJson(boundedFile(manifestPath, MAX_MANIFEST_BYTES, "runtime manifest"), "runtime manifest"),
    "runtime manifest",
  );
  exactKeys(
    manifest,
    [
      "archiveSha256",
      "chunkSha256",
      "entrypoint",
      "fileCount",
      "format",
      "maximumFileBytes",
      "maximumUnpackedBytes",
      "nodeSha256",
      "nodeVersion",
      "packageJsonSha256",
      "platform",
      "pnpmLockSha256",
      "promptfooVersion",
      "schemaVersion",
      "treeSha256",
      "unpackedBytes",
    ],
    "runtime manifest",
  );
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
  extractor.finish();
  if (archiveHash.digest("hex") !== manifest.archiveSha256) {
    throw new RunnerError("runtime archive identity differs from manifest");
  }
  const entrypoint = path.join(root, ...safeRelative(manifest.entrypoint));
  const state = fs.lstatSync(entrypoint);
  if (!state.isFile() || state.isSymbolicLink()) {
    throw new RunnerError("sealed Promptfoo entrypoint is unavailable");
  }
  return entrypoint;
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
  const entrypoint = await extractRuntime(manifestPath, chunkPaths, temporaryRoot);
  const configPath = path.join(promptfoo, "eval-config.json");
  fs.writeFileSync(
    configPath,
    boundedFile(process.env.AI_IP_PROMPTFOO_PATH, MAX_CONFIG_BYTES, "Promptfoo config"),
    { flag: "wx", mode: 0o600 },
  );
  const command = [
    "eval",
    "--config",
    configPath,
    "--output",
    outputPath,
    "--no-cache",
    "--no-progress-bar",
    "--no-table",
    "--no-share",
    "--no-write",
    "--max-concurrency",
    "1",
  ];
  const completed = spawnSync(process.execPath, [entrypoint, ...command], {
    cwd: process.cwd(),
    env: promptfooEnvironment(),
    stdio: ["ignore", "inherit", "inherit"],
  });
  if (completed.error) throw completed.error;
  if (completed.status !== 0) return completed.status ?? 2;
  const parsed = parseResult(
    boundedFile(outputPath, MAX_RESULT_BYTES, "Promptfoo result"),
  );
  writeAll(Number(process.env.AI_IP_RESULT_FD), Buffer.from(canonical(parsed.result)));
  writeAll(Number(process.env.AI_IP_TELEMETRY_FD), Buffer.from(canonical(parsed.telemetry)));
  return 0;
}

module.exports = { nodeVersionSupported, safeRelative, writeAll };

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
