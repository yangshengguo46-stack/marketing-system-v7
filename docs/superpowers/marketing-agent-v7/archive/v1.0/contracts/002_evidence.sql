CREATE TABLE sources (
  workspace_id TEXT NOT NULL, project_id TEXT NOT NULL, id TEXT NOT NULL,
  kind TEXT NOT NULL, identity_json TEXT NOT NULL CHECK(json_valid(identity_json)),
  PRIMARY KEY(workspace_id,project_id,id),
  FOREIGN KEY(workspace_id,project_id) REFERENCES projects(workspace_id,id)
);
CREATE TABLE source_snapshots (
  workspace_id TEXT NOT NULL, project_id TEXT NOT NULL, id TEXT NOT NULL,
  source_id TEXT NOT NULL, object_sha256 TEXT,
  observed_at TEXT NOT NULL,
  coverage TEXT NOT NULL CHECK(coverage IN ('complete_returned_content','partial','snippet_only','metadata_only','unavailable')),
  provenance_json TEXT NOT NULL CHECK(json_valid(provenance_json)),
  PRIMARY KEY(workspace_id,project_id,id),
  FOREIGN KEY(workspace_id,project_id,source_id) REFERENCES sources(workspace_id,project_id,id),
  FOREIGN KEY(workspace_id,object_sha256) REFERENCES objects(workspace_id,plaintext_sha256)
);
CREATE TABLE source_spans (
  workspace_id TEXT NOT NULL, project_id TEXT NOT NULL, id TEXT NOT NULL,
  snapshot_id TEXT NOT NULL,
  locator_json TEXT NOT NULL CHECK(json_valid(locator_json)),
  exact_text TEXT,
  PRIMARY KEY(workspace_id,project_id,id),
  FOREIGN KEY(workspace_id,project_id,snapshot_id) REFERENCES source_snapshots(workspace_id,project_id,id)
);
CREATE TABLE claims (
  workspace_id TEXT NOT NULL, project_id TEXT NOT NULL, id TEXT NOT NULL,
  mission_id TEXT NOT NULL,
  kind TEXT NOT NULL CHECK(kind IN ('userFact','externalEvidence','modelInterpretation','creativeHypothesis','unknown','actualResult')),
  verification_state TEXT NOT NULL CHECK(verification_state IN ('unreviewed','supported','disputed','unsupported','unverifiable')),
  body_json TEXT NOT NULL CHECK(json_valid(body_json)),
  result_receipt_ref TEXT,
  CHECK(kind <> 'actualResult' OR result_receipt_ref IS NOT NULL),
  PRIMARY KEY(workspace_id,project_id,id),
  FOREIGN KEY(workspace_id,project_id,mission_id) REFERENCES missions(workspace_id,project_id,id)
);
CREATE TABLE evidence_links (
  workspace_id TEXT NOT NULL, project_id TEXT NOT NULL,
  claim_id TEXT NOT NULL, span_id TEXT NOT NULL,
  relation TEXT NOT NULL CHECK(relation IN ('supports','contradicts','context_only')),
  PRIMARY KEY(workspace_id,project_id,claim_id,span_id,relation),
  FOREIGN KEY(workspace_id,project_id,claim_id) REFERENCES claims(workspace_id,project_id,id),
  FOREIGN KEY(workspace_id,project_id,span_id) REFERENCES source_spans(workspace_id,project_id,id)
);
CREATE TABLE decisions (
  workspace_id TEXT NOT NULL, project_id TEXT NOT NULL, id TEXT NOT NULL,
  mission_id TEXT NOT NULL, mission_revision INTEGER NOT NULL CHECK(mission_revision > 0),
  body_json TEXT NOT NULL CHECK(json_valid(body_json)),
  created_at TEXT NOT NULL,
  PRIMARY KEY(workspace_id,project_id,id),
  FOREIGN KEY(workspace_id,project_id,mission_id) REFERENCES missions(workspace_id,project_id,id)
);
-- Polymorphic endpoints are checked by the host in the same write transaction.
-- No user/model supplied endpoint is trusted merely because this JSON/row parses.
CREATE TABLE dependencies (
  workspace_id TEXT NOT NULL, project_id TEXT NOT NULL, id TEXT NOT NULL,
  mission_id TEXT NOT NULL,
  dependent_json TEXT NOT NULL CHECK(json_valid(dependent_json)),
  prerequisite_json TEXT NOT NULL CHECK(json_valid(prerequisite_json)),
  strength TEXT NOT NULL CHECK(strength IN ('hard','soft')),
  needs_review INTEGER NOT NULL DEFAULT 0 CHECK(needs_review IN (0,1)),
  PRIMARY KEY(workspace_id,project_id,id),
  FOREIGN KEY(workspace_id,project_id,mission_id) REFERENCES missions(workspace_id,project_id,id)
);
