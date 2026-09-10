CREATE TABLE approvals (
  workspace_id TEXT NOT NULL, project_id TEXT NOT NULL, id TEXT NOT NULL,
  mission_id TEXT NOT NULL,
  account_id TEXT NOT NULL,
  action_kind TEXT NOT NULL,
  payload_sha256 TEXT NOT NULL CHECK(length(payload_sha256)=64),
  status TEXT NOT NULL CHECK(status IN ('pending','approved','denied','revoked','expired','consumed')),
  grant_json TEXT NOT NULL CHECK(json_valid(grant_json)),
  issued_by TEXT, issued_at TEXT, expires_at TEXT,
  CHECK(status <> 'approved' OR (issued_by IS NOT NULL AND issued_at IS NOT NULL AND expires_at IS NOT NULL)),
  PRIMARY KEY(workspace_id,project_id,id),
  FOREIGN KEY(workspace_id,project_id,mission_id) REFERENCES missions(workspace_id,project_id,id)
);
CREATE TABLE action_executions (
  workspace_id TEXT NOT NULL, project_id TEXT NOT NULL, id TEXT NOT NULL,
  mission_id TEXT NOT NULL, approval_id TEXT NOT NULL,
  reservation_id TEXT,
  idempotency_key TEXT NOT NULL,
  payload_sha256 TEXT NOT NULL CHECK(length(payload_sha256)=64),
  state TEXT NOT NULL CHECK(state IN ('prepared','reserved','submitted','succeeded','failed','unknown','cancelled','manual_resolution')),
  provider_operation_id TEXT,
  receipt_json TEXT CHECK(receipt_json IS NULL OR json_valid(receipt_json)),
  created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
  PRIMARY KEY(workspace_id,project_id,id),
  UNIQUE(workspace_id,project_id,idempotency_key),
  FOREIGN KEY(workspace_id,project_id,mission_id) REFERENCES missions(workspace_id,project_id,id),
  FOREIGN KEY(workspace_id,project_id,approval_id) REFERENCES approvals(workspace_id,project_id,id),
  FOREIGN KEY(workspace_id,project_id,reservation_id) REFERENCES budget_reservations(workspace_id,project_id,id)
);
CREATE TABLE observations (
  workspace_id TEXT NOT NULL, project_id TEXT NOT NULL, id TEXT NOT NULL,
  mission_id TEXT NOT NULL, execution_id TEXT,
  source_kind TEXT NOT NULL CHECK(source_kind IN ('platform','user_supplied','computed')),
  observed_at TEXT NOT NULL,
  observation_json TEXT NOT NULL CHECK(json_valid(observation_json)),
  PRIMARY KEY(workspace_id,project_id,id),
  FOREIGN KEY(workspace_id,project_id,mission_id) REFERENCES missions(workspace_id,project_id,id),
  FOREIGN KEY(workspace_id,project_id,execution_id) REFERENCES action_executions(workspace_id,project_id,id)
);
