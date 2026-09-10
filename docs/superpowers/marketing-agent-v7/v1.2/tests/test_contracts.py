"""Local contract checks only: not a Rust/runtime/model/platform acceptance suite."""
from __future__ import annotations
import copy
import json
from pathlib import Path
import sqlite3
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[1]
C = ROOT / 'contracts'
NOW = '2026-09-10T00:00:00Z'
HASH = 'a' * 64

class DatabaseContractTests(unittest.TestCase):
    def setUp(self):
        self.db = sqlite3.connect(':memory:')
        self.db.execute('PRAGMA foreign_keys=ON')
        for file in sorted(C.glob('00*.sql')):
            self.db.executescript(file.read_text())
        self.db.execute('INSERT INTO workspaces VALUES(?,?,?)', ('w','workspace',NOW))
        for p in ('p1','p2'):
            self.db.execute('INSERT INTO projects(workspace_id,id,display_name) VALUES(?,?,?)',('w',p,p))
        self.db.execute('INSERT INTO missions VALUES(?,?,?,?,?,?,?,?,?,?)',('w','p1','m',1,0,'active','Create a draft','{}',NOW,NOW))
        self.db.commit()
    def tearDown(self):
        self.db.close()
    def test_schema_integrity(self):
        self.assertEqual(self.db.execute('PRAGMA integrity_check').fetchone()[0], 'ok')
        self.assertEqual(self.db.execute('PRAGMA foreign_key_check').fetchall(), [])
    def test_cross_project_artifact_rejected(self):
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute('INSERT INTO artifacts VALUES(?,?,?,?,?,?,?)',('w','p2','a','m','copy','draft',NOW))
    def test_invalid_mission_status_rejected(self):
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute("UPDATE missions SET status='research_complete'")
    def test_negative_revision_rejected(self):
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute('UPDATE missions SET revision=-1')
    def test_cas_single_winner(self):
        sql="UPDATE missions SET revision=revision+1 WHERE workspace_id=? AND project_id=? AND id=? AND revision=?"
        self.assertEqual(self.db.execute(sql,('w','p1','m',1)).rowcount,1)
        self.assertEqual(self.db.execute(sql,('w','p1','m',1)).rowcount,0)
    def test_duplicate_command_unique(self):
        row=('w','p1','c1',HASH,'{}',NOW)
        self.db.execute('INSERT INTO command_dedup VALUES(?,?,?,?,?,?)',row)
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute('INSERT INTO command_dedup VALUES(?,?,?,?,?,?)',row)
    def test_mission_and_event_rollback_together(self):
        self.db.execute('BEGIN IMMEDIATE')
        self.db.execute('UPDATE missions SET revision=2')
        self.db.execute('INSERT INTO events VALUES(?,?,?,?,?,?,?,?,?)',('w','p1','m',1,'e',2,'mission.updated','{}',NOW))
        self.db.rollback()
        self.assertEqual(self.db.execute('SELECT revision FROM missions').fetchone()[0],1)
        self.assertEqual(self.db.execute('SELECT count(*) FROM events').fetchone()[0],0)
    def test_actual_result_needs_receipt(self):
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute('INSERT INTO claims VALUES(?,?,?,?,?,?,?,?)',('w','p1','c','m','actualResult','unreviewed','{}',None))
    def test_arbitrary_json_rejected(self):
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute("UPDATE missions SET contract_json='not-json'")
    def test_invalid_digest_rejected(self):
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute('INSERT INTO objects VALUES(?,?,?,?,?,?,?)',('w','z'*64,1,'text/plain','x',None,NOW))
    def test_released_requires_timestamp(self):
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute('INSERT INTO release_candidates VALUES(?,?,?,?,?,?,?,?,?,?,?)',('w','p1','r','m',1,0,HASH,HASH,'released',NOW,None))
    def test_approval_requires_issuer(self):
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute('INSERT INTO approvals VALUES(?,?,?,?,?,?,?,?,?,?,?,?)',('w','p1','ap','m','acc','publish',HASH,'approved','{}',None,None,None))
    def test_missing_artifact_version_rejected(self):
        self.db.execute('INSERT INTO release_candidates VALUES(?,?,?,?,?,?,?,?,?,?,?)',('w','p1','r','m',1,0,HASH,HASH,'proposed',NOW,None))
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute('INSERT INTO release_items VALUES(?,?,?,?)',('w','p1','r','missing'))
    def test_budget_reserve_conditional_update(self):
        self.db.execute('INSERT INTO budget_accounts VALUES(?,?,?,?,?,?,?,?,?)',('w','p1','m','CNY',100,0,0,0,1))
        sql="""UPDATE budget_accounts SET reserved_micros=reserved_micros+?,revision=revision+1
        WHERE workspace_id=? AND project_id=? AND mission_id=? AND currency=? AND blocked=0
        AND limit_micros-settled_micros-reserved_micros>=?"""
        self.assertEqual(self.db.execute(sql,(60,'w','p1','m','CNY',60)).rowcount,1)
        self.assertEqual(self.db.execute(sql,(60,'w','p1','m','CNY',60)).rowcount,0)

class JsonContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        try:
            import jsonschema
        except ImportError:
            raise unittest.SkipTest('Install jsonschema to run JSON Schema validation.')
        cls.js = jsonschema
        cls.schema = json.loads((C/'review.schema.json').read_text())
        cls.sample = json.loads((C/'review.example.json').read_text())
    def test_schema_definition(self):
        self.js.Draft202012Validator.check_schema(self.schema)
    def test_example_valid(self):
        self.js.validate(self.sample,self.schema)
    def test_revision_requires_findings(self):
        bad=copy.deepcopy(self.sample); bad['findings']=[]
        with self.assertRaises(self.js.ValidationError): self.js.validate(bad,self.schema)
    def test_unknown_field_rejected(self):
        bad=copy.deepcopy(self.sample); bad['approved']=True
        with self.assertRaises(self.js.ValidationError): self.js.validate(bad,self.schema)
    def test_finding_bound(self):
        bad=copy.deepcopy(self.sample); bad['findings']=bad['findings']*17
        with self.assertRaises(self.js.ValidationError): self.js.validate(bad,self.schema)
    def test_allow_cannot_include_blocking_finding(self):
        bad=copy.deepcopy(self.sample); bad['verdict']='allow'
        with self.assertRaises(self.js.ValidationError): self.js.validate(bad,self.schema)

class ConfigurationTests(unittest.TestCase):
    def test_safe_example(self):
        config=tomllib.loads((C/'defaults.example.toml').read_text())
        self.assertFalse(config['models']['production_primary']['ready'])
        self.assertFalse(config['models']['review']['allow_research_reference_model'])
        self.assertFalse(config['execution']['external_publish_enabled'])
        self.assertFalse(config['execution']['retry_unknown_external_write'])
        self.assertEqual(config['transport']['kind'],'stdio')

if __name__ == '__main__':
    unittest.main(verbosity=2)
