"use strict";

const crypto = require("node:crypto");
const { TextDecoder } = require("node:util");

const MAX_EVENTS = 1024;
const MAX_EVIDENCE_ITEM_BYTES = 256 * 1024;
const MAX_FINAL_RESPONSE_BYTES = 1024 * 1024;
const MAX_TRAJECTORY_BYTES = 4 * 1024 * 1024;
const MAX_JSON_DEPTH = 64;
const MAX_JSON_NODES = 10000;
const MAX_SAFE_INTEGER = BigInt(Number.MAX_SAFE_INTEGER);

class PromptfooResultError extends Error {}

function sha256(payload) {
  return crypto.createHash("sha256").update(payload).digest("hex");
}

function canonical(value) {
  if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
  if (value !== null && typeof value === "object") {
    return `{${Object.keys(value)
      .sort()
      .map((key) => `${JSON.stringify(key)}:${canonical(value[key])}`)
      .join(",")}}`;
  }
  return JSON.stringify(value);
}

function boundedJson(payload, label) {
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
      string();
      return;
    }
    if (current === "[") {
      index += 1;
      whitespace();
      if (text[index] === "]") {
        index += 1;
        return;
      }
      while (true) {
        value(depth + 1);
        whitespace();
        if (text[index] === "]") {
          index += 1;
          return;
        }
        if (text[index] !== ",") throw new PromptfooResultError(`${label} is not valid strict JSON`);
        index += 1;
      }
    }
    if (current === "{") {
      index += 1;
      whitespace();
      if (text[index] === "}") {
        index += 1;
        return;
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
        value(depth + 1);
        whitespace();
        if (text[index] === "}") {
          index += 1;
          return;
        }
        if (text[index] !== ",") throw new PromptfooResultError(`${label} is not valid strict JSON`);
        index += 1;
      }
    }
    for (const literal of ["true", "false", "null"]) {
      if (text.startsWith(literal, index)) {
        index += literal.length;
        return;
      }
    }
    const match = /^-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?/.exec(text.slice(index));
    if (!match) throw new PromptfooResultError(`${label} is not valid strict JSON`);
    const token = match[0];
    index += token.length;
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
  };
  try {
    value(1);
    whitespace();
    if (index !== text.length) throw new PromptfooResultError(`${label} is not valid strict JSON`);
    return JSON.parse(text);
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
    if (item === null || typeof item === "boolean") return;
    if (typeof item === "string") {
      for (let index = 0; index < item.length; index += 1) {
        const code = item.charCodeAt(index);
        if (code >= 0xd800 && code <= 0xdbff) {
          const next = item.charCodeAt(index + 1);
          if (next < 0xdc00 || next > 0xdfff) {
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
  const output = boundedJson(Buffer.from(response.output), "Promptfoo output");
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
    typeof raw.finalResponse !== "string"
  ) {
    throw new PromptfooResultError("Codex raw events are required");
  }
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

module.exports = { canonical, parseResult };
