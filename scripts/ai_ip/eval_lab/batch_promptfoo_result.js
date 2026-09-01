"use strict";
const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
const { TextDecoder } = require("node:util");
const MAX_EVENTS = 1024;
const MAX_EVIDENCE_ITEM_BYTES = 256 * 1024;
const MAX_FINAL_RESPONSE_BYTES = 1024 * 1024;
const MAX_TRAJECTORY_BYTES = 4 * 1024 * 1024;
const MAX_JSON_DEPTH = 64;
const MAX_JSON_NODES = 10000;
const MAX_SAFE_INTEGER = BigInt(Number.MAX_SAFE_INTEGER);
class PromptfooResultError extends Error {}
class RunnerError extends Error {
  constructor(message, options) { super(message, options); this.name = "RunnerError"; }
}
class NonIntegralJsonNumber {}
function nodeVersionSupported(value) {
  const matched = /^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.exec(value);
  if (!matched) return false;
  const version = matched.slice(1).map(Number);
  return version[0] > 22 ||
    (version[0] === 22 && (version[1] > 22 || (version[1] === 22 && version[2] >= 0)));
}
function sha256(payload) {
  return crypto.createHash("sha256").update(payload).digest("hex");
}
function boundedFile(filePath, maximum, label) {
  const descriptor = fs.openSync(filePath, fs.constants.O_RDONLY | (fs.constants.O_NOFOLLOW || 0));
  try {
    const before = fs.fstatSync(descriptor);
    if (!before.isFile() || before.size > maximum) {
      throw new RunnerError(`${label} is not a bounded regular file`);
    }
    const payload = Buffer.alloc(before.size);
    let offset = 0;
    while (offset < payload.length) {
      const amount = fs.readSync(
        descriptor, payload, offset, payload.length - offset, offset,
      );
      if (amount < 1) throw new RunnerError(`${label} ended before its declared size`);
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
function safeRelative(value) {
  const encoded = typeof value === "string" ? Buffer.from(value, "utf8") : null;
  if (
    typeof value !== "string" || value.length < 1 || encoded.toString("utf8") !== value ||
    encoded.length > 4096 || value.includes("\\") || /[\x00-\x1f\x7f]/.test(value) ||
    path.posix.isAbsolute(value)
  ) {
    throw new RunnerError("runtime archive path is unsafe");
  }
  const parts = value.split("/");
  if (parts.length > 128 || parts.some((part) => part === "" || part === "." || part === "..")) {
    throw new RunnerError("runtime archive path traversal is prohibited");
  }
  return parts;
}
function writeAllAt(fd, payload, position, writer = fs.writeSync, label = "runtime file") {
  let offset = 0;
  while (offset < payload.length) {
    let amount;
    try {
      amount = position === null
        ? writer(fd, payload, offset, payload.length - offset)
        : writer(fd, payload, offset, payload.length - offset, position + offset);
    } catch (error) {
      if (error?.code === "EINTR") continue;
      throw error;
    }
    if (!Number.isSafeInteger(amount) || amount < 1 || amount > payload.length - offset) {
      throw new RunnerError(`${label} write made no progress`);
    }
    offset += amount;
  }
}
function writeAll(fd, payload, writer = fs.writeSync) {
  writeAllAt(fd, payload, null, writer, "pipe");
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
function ensureParents(root, parts) {
  let current = root;
  for (const part of parts.slice(0, -1)) {
    current = path.join(current, part);
    try {
      fs.mkdirSync(current, { mode: 0o700 });
    } catch (error) {
      if (error.code !== "EEXIST") throw error;
      const state = fs.lstatSync(current);
      if (!state.isDirectory() || state.isSymbolicLink())
        throw new RunnerError("runtime archive parent is unsafe");
    }
  }
}
function sameIdentity(left, right) {
  return left.dev === right.dev && left.ino === right.ino;
}
function descriptorDigest(descriptor, size) {
  const digest = crypto.createHash("sha256");
  const buffer = Buffer.alloc(Math.min(1024 * 1024, Math.max(1, size)));
  for (let offset = 0; offset < size;) {
    let amount;
    try {
      amount = fs.readSync(descriptor, buffer, 0, Math.min(buffer.length, size - offset), offset);
    } catch (error) {
      if (error?.code === "EINTR") continue;
      throw error;
    }
    if (amount < 1) throw new RunnerError("runtime file ended during attestation");
    digest.update(buffer.subarray(0, amount));
    offset += amount;
  }
  return digest.digest("hex");
}
function canonical(value) {
  if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
  if (value !== null && typeof value === "object") {
    return `{${Object.keys(value)
      .sort((left, right) => Buffer.compare(Buffer.from(left, "utf8"), Buffer.from(right, "utf8")))
      .map((key) => `${JSON.stringify(key)}:${canonical(value[key])}`)
      .join(",")}}`;
  }
  return JSON.stringify(value);
}

function boundedJson(payload, label, numberContract = "finite") {
  let shapeDepth = 0;
  let shapeNodes = 1;
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
      shapeDepth += 1;
      shapeNodes += 1;
      if (shapeDepth > MAX_JSON_DEPTH) throw new PromptfooResultError(`${label} exceeds JSON depth`);
    } else if (byte === 0x5d || byte === 0x7d) {
      shapeDepth -= 1;
      if (shapeDepth < 0) throw new PromptfooResultError(`${label} has invalid JSON shape`);
    } else if (byte === 0x2c || byte === 0x3a) {
      shapeNodes += 1;
      if (shapeNodes > MAX_JSON_NODES) throw new PromptfooResultError(`${label} exceeds JSON nodes`);
    }
  }
  if (inString || shapeDepth !== 0) throw new PromptfooResultError(`${label} has invalid JSON shape`);
  let text;
  try {
    text = new TextDecoder("utf-8", { fatal: true }).decode(payload);
  } catch (error) {
    throw new PromptfooResultError(`${label} is not valid strict JSON`, { cause: error });
  }
  let index = 0;
  let nodes = 0;
  const whitespace = () => {
    while (index < text.length && /[\t\n\r ]/.test(text[index])) index += 1;
  };
  const string = () => {
    const start = index;
    index += 1;
    while (index < text.length) {
      const code = text.charCodeAt(index);
      if (code === 0x22) {
        index += 1;
        return JSON.parse(text.slice(start, index));
      }
      if (code < 0x20) throw new PromptfooResultError(`${label} is not valid strict JSON`);
      if (code === 0x5c) {
        index += 1;
        if (index >= text.length || !/["\\/bfnrtu]/.test(text[index])) {
          throw new PromptfooResultError(`${label} is not valid strict JSON`);
        }
        if (text[index] === "u") {
          if (!/^[0-9a-fA-F]{4}$/.test(text.slice(index + 1, index + 5))) {
            throw new PromptfooResultError(`${label} is not valid strict JSON`);
          }
          index += 4;
        }
      }
      index += 1;
    }
    throw new PromptfooResultError(`${label} is not valid strict JSON`);
  };
  const value = (depth) => {
    nodes += 1;
    if (nodes > MAX_JSON_NODES) throw new PromptfooResultError(`${label} exceeds JSON nodes`);
    if (depth > MAX_JSON_DEPTH) throw new PromptfooResultError(`${label} exceeds JSON depth`);
    whitespace();
    const current = text[index];
    if (current === '"') {
      return string();
    }
    if (current === "[") {
      const result = [];
      index += 1;
      whitespace();
      if (text[index] === "]") {
        index += 1;
        return result;
      }
      while (true) {
        result.push(value(depth + 1));
        whitespace();
        if (text[index] === "]") {
          index += 1;
          return result;
        }
        if (text[index] !== ",") throw new PromptfooResultError(`${label} is not valid strict JSON`);
        index += 1;
      }
    }
    if (current === "{") {
      const result = Object.create(null);
      index += 1;
      whitespace();
      if (text[index] === "}") {
        index += 1;
        return result;
      }
      const keys = new Set();
      while (true) {
        whitespace();
        if (text[index] !== '"') throw new PromptfooResultError(`${label} is not valid strict JSON`);
        const key = string();
        if (keys.has(key)) throw new PromptfooResultError(`${label} has a duplicate JSON key`);
        keys.add(key);
        whitespace();
        if (text[index] !== ":") throw new PromptfooResultError(`${label} is not valid strict JSON`);
        index += 1;
        result[key] = value(depth + 1);
        whitespace();
        if (text[index] === "}") {
          index += 1;
          return result;
        }
        if (text[index] !== ",") throw new PromptfooResultError(`${label} is not valid strict JSON`);
        index += 1;
      }
    }
    for (const [literal, result] of [["true", true], ["false", false], ["null", null]]) {
      if (text.startsWith(literal, index)) {
        index += literal.length;
        return result;
      }
    }
    const match = /^-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?/.exec(text.slice(index));
    if (!match) throw new PromptfooResultError(`${label} is not valid strict JSON`);
    const token = match[0];
    index += token.length;
    if (numberContract === "integer-evidence" && (token.includes(".") || /[eE]/.test(token))) {
      throw new PromptfooResultError(`${label} contains a non-integral JSON number`);
    }
    if (!token.includes(".") && !/[eE]/.test(token) && BigInt(token) > MAX_SAFE_INTEGER) {
      throw new PromptfooResultError(`${label} contains an unsafe JSON integer`);
    }
    if (!token.includes(".") && !/[eE]/.test(token) && BigInt(token) < -MAX_SAFE_INTEGER) {
      throw new PromptfooResultError(`${label} contains an unsafe JSON integer`);
    }
    const numeric = Number(token);
    if (!Number.isFinite(numeric) || (Number.isInteger(numeric) && !Number.isSafeInteger(numeric))) {
      throw new PromptfooResultError(`${label} contains a non-finite JSON number`);
    }
    if (token.includes(".") || /[eE]/.test(token)) return new NonIntegralJsonNumber();
    return numeric;
  };
  try {
    const result = value(1);
    whitespace();
    if (index !== text.length) throw new PromptfooResultError(`${label} is not valid strict JSON`);
    return result;
  } catch (error) {
    if (error instanceof PromptfooResultError) throw error;
    throw new PromptfooResultError(`${label} is not valid JSON`, { cause: error });
  }
}

function object(value, label) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new PromptfooResultError(`${label} must be an object`);
  }
  return value;
}

function nonnegative(value, label) {
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new PromptfooResultError(`${label} must be a nonnegative integer`);
  }
  return value;
}

function usageLayer(value, label, requireRequests) {
  const usage = object(value, label);
  const fields = ["prompt", "completion", "cached", "total"];
  if (requireRequests) fields.push("numRequests");
  const allowed = requireRequests ? [...fields, "assertions"] : fields;
  const actual = Object.keys(usage).sort();
  if (
    actual.join("\0") !== [...fields].sort().join("\0") &&
    actual.join("\0") !== [...allowed].sort().join("\0")
  ) {
    throw new PromptfooResultError(`${label} has invalid token usage fields`);
  }
  const result = Object.fromEntries(
    fields.map((field) => [field, nonnegative(usage[field], `${label} ${field}`)]),
  );
  if (result.total !== result.prompt + result.completion) {
    throw new PromptfooResultError(`${label} total is inconsistent`);
  }
  return result;
}

function boundedEvidence(value, maximum, label) {
  const validate = (item) => {
    if (item instanceof NonIntegralJsonNumber) {
      throw new PromptfooResultError(`${label} contains unsupported evidence values`);
    }
    if (item === null || typeof item === "boolean") return;
    if (typeof item === "string") {
      for (let index = 0; index < item.length; index += 1) {
        const code = item.charCodeAt(index);
        if (code >= 0xd800 && code <= 0xdbff) {
          const next = item.charCodeAt(index + 1);
          if (!(next >= 0xdc00 && next <= 0xdfff)) {
            throw new PromptfooResultError(`${label} contains invalid Unicode`);
          }
          index += 1;
        } else if (code >= 0xdc00 && code <= 0xdfff) {
          throw new PromptfooResultError(`${label} contains invalid Unicode`);
        }
      }
      return;
    }
    if (Number.isSafeInteger(item)) return;
    if (Array.isArray(item)) {
      item.forEach(validate);
      return;
    }
    if (typeof item === "object") {
      Object.entries(item).forEach(([key, child]) => {
        validate(key);
        validate(child);
      });
      return;
    }
    throw new PromptfooResultError(`${label} contains unsupported evidence values`);
  };
  validate(value);
  if (Buffer.byteLength(canonical(value)) > maximum) {
    throw new PromptfooResultError(`${label} exceeds its evidence byte bound`);
  }
}

function parseResult(payload) {
  const root = object(boundedJson(payload, "Promptfoo result"), "Promptfoo result");
  const results = object(root.results, "Promptfoo results");
  if (!Array.isArray(results.results) || results.results.length !== 1) {
    throw new PromptfooResultError("Promptfoo must contain exactly one result row");
  }
  const row = object(results.results[0], "Promptfoo row");
  const response = object(row.response, "Promptfoo response");
  if (
    (row.error !== undefined && row.error !== null && row.error !== "") ||
    (response.error !== undefined && response.error !== null && response.error !== "")
  ) {
    throw new PromptfooResultError("Promptfoo reported a provider error");
  }
  if (typeof response.output !== "string") {
    throw new PromptfooResultError("Promptfoo output must be a string");
  }
  const metadata = object(response.metadata, "Promptfoo metadata");
  const codex = object(metadata.codexAppServer, "Codex metadata");
  if (
    typeof codex.threadId !== "string" ||
    !codex.threadId.trim() ||
    typeof codex.turnId !== "string" ||
    !codex.turnId.trim()
  ) {
    throw new PromptfooResultError("Codex thread and turn IDs are required");
  }
  if (!Array.isArray(codex.items) || !codex.items.length || codex.items.length > MAX_EVENTS) {
    throw new PromptfooResultError("Codex trajectory items are required");
  }
  if (typeof response.raw !== "string") throw new PromptfooResultError("Codex raw events are required");
  const raw = object(boundedJson(Buffer.from(response.raw), "Codex raw response"), "Codex raw response");
  if (
    !Array.isArray(raw.notifications) ||
    !raw.notifications.length ||
    raw.notifications.length > MAX_EVENTS ||
    !Array.isArray(raw.items) ||
    !raw.items.length ||
    raw.items.length > MAX_EVENTS ||
    typeof raw.output !== "string" ||
    typeof raw.finalResponse !== "string"
  ) {
    throw new PromptfooResultError("Codex raw events are required");
  }
  if (response.output !== raw.output || response.output !== raw.finalResponse) {
    throw new PromptfooResultError("Promptfoo final response differs across retained output fields");
  }
  const output = boundedJson(
    Buffer.from(response.output),
    "Promptfoo output",
    "integer-evidence",
  );
  boundedEvidence(output, MAX_FINAL_RESPONSE_BYTES, "Promptfoo final response");
  const responseUsage = usageLayer(response.tokenUsage, "response token usage", false);
  const rowUsage = usageLayer(row.tokenUsage, "row token usage", true);
  for (const [field, value] of Object.entries(responseUsage)) {
    if (rowUsage[field] !== value) {
      throw new PromptfooResultError(`token usage differs for ${field}`);
    }
  }
  if (rowUsage.numRequests < 1) throw new PromptfooResultError("request count must be positive");
  for (const [label, values] of [
    ["Codex trajectory item", codex.items],
    ["Codex raw item", raw.items],
    ["Codex notification", raw.notifications],
  ]) {
    values.forEach((value) => boundedEvidence(value, MAX_EVIDENCE_ITEM_BYTES, label));
  }
  boundedEvidence(raw.finalResponse, MAX_FINAL_RESPONSE_BYTES, "Codex final response");
  const trajectory = {
    finalResponse: raw.finalResponse,
    items: codex.items,
    notifications: raw.notifications,
    rawItems: raw.items,
    responseUsage,
    rowUsage,
  };
  boundedEvidence(trajectory, MAX_TRAJECTORY_BYTES, "Codex retained evidence");
  const trajectoryBytes = Buffer.from(canonical(trajectory));
  return {
    result: {
      metadata: {
        costEvidence: { costCny: 0, sourceSha256: sha256(trajectoryBytes) },
        requestCount: rowUsage.numRequests,
        threadId: codex.threadId,
        trajectory,
        turnId: codex.turnId,
        usage: {
          inputTokens: responseUsage.prompt,
          outputTokens: responseUsage.completion,
          totalTokens: responseUsage.total,
        },
      },
      output,
    },
    telemetry: {
      costCny: 0,
      inputTokens: responseUsage.prompt,
      outputTokens: responseUsage.completion,
      requestCount: rowUsage.numRequests,
    },
  };
}

module.exports = {
  RunnerError, boundedFile, canonical, descriptorDigest, ensureParents, isolatedDirectory,
  isolatedOutput, nodeVersionSupported, parseResult, safeRelative, sameIdentity, sha256, writeAll,
  writeAllAt,
};
