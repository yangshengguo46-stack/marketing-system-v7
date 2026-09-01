"use strict";

const crypto = require("node:crypto");

const MAX_EVENTS = 1024;
const MAX_JSON_DEPTH = 64;
const MAX_JSON_NODES = 10000;

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
      if (depth > MAX_JSON_DEPTH) throw new PromptfooResultError(`${label} exceeds JSON depth`);
    } else if (byte === 0x5d || byte === 0x7d) {
      depth -= 1;
      if (depth < 0) throw new PromptfooResultError(`${label} has invalid JSON shape`);
    } else if (byte === 0x2c || byte === 0x3a) {
      nodes += 1;
      if (nodes > MAX_JSON_NODES) throw new PromptfooResultError(`${label} exceeds JSON nodes`);
    }
  }
  if (inString || depth !== 0) throw new PromptfooResultError(`${label} has invalid JSON shape`);
  try {
    return JSON.parse(payload.toString("utf8"));
  } catch (error) {
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
  const fields = ["prompt", "completion", "total"];
  if (requireRequests || Object.hasOwn(usage, "numRequests")) fields.push("numRequests");
  const result = Object.fromEntries(
    fields.map((field) => [field, nonnegative(usage[field], `${label} ${field}`)]),
  );
  if (result.total !== result.prompt + result.completion) {
    throw new PromptfooResultError(`${label} total is inconsistent`);
  }
  return result;
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
  if (!Array.isArray(raw.notifications) || !raw.notifications.length || raw.notifications.length > MAX_EVENTS) {
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
  return {
    result: {
      metadata: {
        costEvidence: { costCny: 0, sourceSha256: sha256(payload) },
        requestCount: rowUsage.numRequests,
        threadId: codex.threadId,
        trajectory: raw.notifications,
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
