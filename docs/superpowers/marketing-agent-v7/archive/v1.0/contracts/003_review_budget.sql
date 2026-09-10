CREATE TABLE budget_accounts (
  workspace_id TEXT NOT NULL, project_id TEXT NOT NULL, mission_id TEXT NOT NULL,
  currency TEXT NOT NULL,
  limit_micros INTEGER NOT NULL CHECK(limit_micros >= 0),
  reserved_micros INTEGER NOT NULL DEFAULT 0 CHECK(reserved_micros >= 0),
  settled_micros INTEGER NOT NULL DEFAULT 0 CHECK(settled_micros >= 0),
  -- A provider overrun is recorded, not rejected or hidden: blocked=1 pauses new spend.
  blocked INTEGER NOT NULL DEFAULT 0 CHECK(blocked IN (0,1)),
  revision INTEGER NOT NULL DEFAULT 1 CHECK(revision > 0),
  PRIMARY KEY(workspace_id,project_id,mission_id,currency),
  FOREIGN KEY(workspace_id,project_id,mission_id) REFERENCES missions(workspace_id,project_id,id)
);
CREATE TABLE budget_reservations (
  workspace_id TEXT NOT NULL, project_id TEXT NOT NULL, id TEXT NOT NULL,
  mission_id TEXT NOT NULL, currency TEXT NOT NULL,
  amount_micros INTEGER NOT NULL CHECK(amount_micros >= 0),
  actual_micros INTEGER CHECK(actual_micros IS NULL OR actual_micros >= 0),
  status TEXT NOT NULL CHECK(status IN ('reserved','submitted','settled','released','unreconciled')),
  request_sha256 TEXT NOT NULL,
  created_at TEXT NOT NULL,
  PRIMARY KEY(workspace_id,project_id,id),
  FOREIGN KEY(workspace_id,project_id,mission_id,currency)
    REFERENCES budget_accounts(workspace_id,project_id,mission_id,currency)
);
CREATE TABLE release_candidates (
  workspace_id TEXT NOT NULL, project_id TEXT NOT NULL, id TEXT NOT NULL,
  mission_id TEXT NOT NULL, mission_revision INTEGER NOT NULL,
  input_epoch INTEGER NOT NULL CHECK(input_epoch >= 0),
  candidate_sha256 TEXT NOT NULL CHECK(length(candidate_sha256)=64),
  acceptance_sha256 TEXT NOT NULL CHECK(length(acceptance_sha256)=64),
  status TEXT NOT NULL CHECK(status IN ('proposed','reviewing','needs_revision','blocked','stale','released')),
  created_at TEXT NOT NULL,
  released_at TEXT,
  CHECK(status <> 'released' OR released_at IS NOT NULL),
  PRIMARY KEY(workspace_id,project_id,id),
  FOREIGN KEY(workspace_id,project_id,mission_id) REFERENCES missions(workspace_id,project_id,id)
);
CREATE TABLE release_items (
  workspace_id TEXT NOT NULL, project_id TEXT NOT NULL,
  candidate_id TEXT NOT NULL, artifact_version_id TEXT NOT NULL,
  PRIMARY KEY(workspace_id,project_id,candidate_id,artifact_version_id),
  FOREIGN KEY(workspace_id,project_id,candidate_id) REFERENCES release_candidates(workspace_id,project_id,id),
  FOREIGN KEY(workspace_id,project_id,artifact_version_id) REFERENCES artifact_versions(workspace_id,project_id,version_id)
);
CREATE TABLE reviews (
  workspace_id TEXT NOT NULL, project_id TEXT NOT NULL, id TEXT NOT NULL,
  candidate_id TEXT NOT NULL, candidate_sha256 TEXT NOT NULL,
  attempt INTEGER NOT NULL CHECK(attempt >= 0),
  verdict TEXT NOT NULL CHECK(verdict IN ('allow','revise','wait','block','unavailable')),
  report_json TEXT NOT NULL CHECK(json_valid(report_json)),
  created_at TEXT NOT NULL,
  PRIMARY KEY(workspace_id,project_id,id),
  UNIQUE(workspace_id,project_id,candidate_id,attempt),
  FOREIGN KEY(workspace_id,project_id,candidate_id) REFERENCES release_candidates(workspace_id,project_id,id)
);
