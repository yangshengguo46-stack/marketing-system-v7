-- Target schema draft. Apply on a copy first; production migration runner owns transactions.
-- Enable foreign_keys for EVERY connection before BEGIN. WAL is configured outside migrations.
PRAGMA foreign_keys = ON;

CREATE TABLE workspaces (
  id TEXT PRIMARY KEY,
  display_name TEXT NOT NULL,
  created_at TEXT NOT NULL
);
CREATE TABLE projects (
  workspace_id TEXT NOT NULL,
  id TEXT NOT NULL,
  display_name TEXT NOT NULL,
  revision INTEGER NOT NULL DEFAULT 1 CHECK(revision > 0),
  metadata_json TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(metadata_json)),
  PRIMARY KEY(workspace_id,id),
  FOREIGN KEY(workspace_id) REFERENCES workspaces(id)
);
CREATE TABLE missions (
  workspace_id TEXT NOT NULL,
  project_id TEXT NOT NULL,
  id TEXT NOT NULL,
  revision INTEGER NOT NULL DEFAULT 1 CHECK(revision > 0),
  input_epoch INTEGER NOT NULL DEFAULT 0 CHECK(input_epoch >= 0),
  status TEXT NOT NULL CHECK(status IN
    ('active','reviewing','delivered','waiting_input','waiting_approval','blocked','paused','cancelled','failed')),
  objective TEXT NOT NULL CHECK(length(trim(objective)) > 0),
  contract_json TEXT NOT NULL CHECK(json_valid(contract_json)),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY(workspace_id,project_id,id),
  FOREIGN KEY(workspace_id,project_id) REFERENCES projects(workspace_id,id)
);
CREATE TABLE thread_bindings (
  workspace_id TEXT NOT NULL,
  project_id TEXT NOT NULL,
  thread_id TEXT NOT NULL,
  mission_id TEXT NOT NULL,
  revision INTEGER NOT NULL DEFAULT 1 CHECK(revision > 0),
  access_mode TEXT NOT NULL CHECK(access_mode IN ('read_only','lead_writer')),
  PRIMARY KEY(workspace_id,thread_id),
  FOREIGN KEY(workspace_id,project_id,mission_id) REFERENCES missions(workspace_id,project_id,id)
);
CREATE TABLE mission_leases (
  workspace_id TEXT NOT NULL,
  project_id TEXT NOT NULL,
  mission_id TEXT NOT NULL,
  holder_id TEXT NOT NULL,
  fencing_token INTEGER NOT NULL CHECK(fencing_token > 0),
  expires_at TEXT NOT NULL,
  PRIMARY KEY(workspace_id,project_id,mission_id),
  FOREIGN KEY(workspace_id,project_id,mission_id) REFERENCES missions(workspace_id,project_id,id)
);
CREATE TABLE objects (
  workspace_id TEXT NOT NULL,
  plaintext_sha256 TEXT NOT NULL CHECK(length(plaintext_sha256)=64 AND plaintext_sha256 NOT GLOB '*[^0-9a-f]*'),
  byte_length INTEGER NOT NULL CHECK(byte_length >= 0),
  mime_type TEXT NOT NULL,
  storage_key TEXT NOT NULL,
  encryption_json TEXT CHECK(encryption_json IS NULL OR json_valid(encryption_json)),
  created_at TEXT NOT NULL,
  PRIMARY KEY(workspace_id,plaintext_sha256),
  FOREIGN KEY(workspace_id) REFERENCES workspaces(id)
);
CREATE TABLE artifacts (
  workspace_id TEXT NOT NULL,
  project_id TEXT NOT NULL,
  id TEXT NOT NULL,
  mission_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  title TEXT NOT NULL,
  created_at TEXT NOT NULL,
  PRIMARY KEY(workspace_id,project_id,id),
  FOREIGN KEY(workspace_id,project_id,mission_id) REFERENCES missions(workspace_id,project_id,id)
);
CREATE TABLE artifact_versions (
  workspace_id TEXT NOT NULL,
  project_id TEXT NOT NULL,
  artifact_id TEXT NOT NULL,
  version INTEGER NOT NULL CHECK(version > 0),
  version_id TEXT NOT NULL,
  parent_version INTEGER,
  mission_revision INTEGER NOT NULL CHECK(mission_revision > 0),
  object_sha256 TEXT NOT NULL,
  metadata_json TEXT NOT NULL CHECK(json_valid(metadata_json)),
  created_at TEXT NOT NULL,
  PRIMARY KEY(workspace_id,project_id,artifact_id,version),
  UNIQUE(workspace_id,project_id,version_id),
  CHECK(parent_version IS NULL OR parent_version < version),
  FOREIGN KEY(workspace_id,project_id,artifact_id) REFERENCES artifacts(workspace_id,project_id,id),
  FOREIGN KEY(workspace_id,project_id,artifact_id,parent_version)
    REFERENCES artifact_versions(workspace_id,project_id,artifact_id,version),
  FOREIGN KEY(workspace_id,object_sha256) REFERENCES objects(workspace_id,plaintext_sha256)
);
CREATE TABLE events (
  workspace_id TEXT NOT NULL,
  project_id TEXT NOT NULL,
  mission_id TEXT NOT NULL,
  event_seq INTEGER NOT NULL CHECK(event_seq > 0),
  event_id TEXT NOT NULL,
  mission_revision INTEGER NOT NULL CHECK(mission_revision > 0),
  event_type TEXT NOT NULL,
  payload_json TEXT NOT NULL CHECK(json_valid(payload_json)),
  created_at TEXT NOT NULL,
  PRIMARY KEY(workspace_id,project_id,mission_id,event_seq),
  UNIQUE(workspace_id,event_id),
  FOREIGN KEY(workspace_id,project_id,mission_id) REFERENCES missions(workspace_id,project_id,id)
);
CREATE TABLE command_dedup (
  workspace_id TEXT NOT NULL,
  project_id TEXT NOT NULL,
  command_id TEXT NOT NULL,
  request_sha256 TEXT NOT NULL CHECK(length(request_sha256)=64),
  response_json TEXT NOT NULL CHECK(json_valid(response_json)),
  created_at TEXT NOT NULL,
  PRIMARY KEY(workspace_id,project_id,command_id),
  FOREIGN KEY(workspace_id,project_id) REFERENCES projects(workspace_id,id)
);
CREATE TABLE legacy_imports (
  workspace_id TEXT NOT NULL,
  project_id TEXT NOT NULL,
  legacy_work_id TEXT NOT NULL,
  original_sha256 TEXT NOT NULL CHECK(length(original_sha256)=64),
  mission_id TEXT NOT NULL,
  report_json TEXT NOT NULL CHECK(json_valid(report_json)),
  imported_at TEXT NOT NULL,
  PRIMARY KEY(workspace_id,project_id,legacy_work_id,original_sha256),
  FOREIGN KEY(workspace_id,project_id,mission_id) REFERENCES missions(workspace_id,project_id,id)
);
CREATE INDEX artifacts_by_mission ON artifacts(workspace_id,project_id,mission_id);
