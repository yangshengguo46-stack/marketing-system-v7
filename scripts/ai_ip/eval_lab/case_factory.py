import hashlib
import os
import stat
from datetime import UTC, date, datetime, time
from pathlib import Path

if __package__:
    from .contracts import (
        CaseCompilationReceipt,
        LabContractError,
        _load_exact_json_bytes,
        sha256_json,
        validate_contract,
    )
    from .private_fs import PrivateRoot
else:
    from contracts import (
        CaseCompilationReceipt,
        LabContractError,
        _load_exact_json_bytes,
        sha256_json,
        validate_contract,
    )
    from private_fs import PrivateRoot


CASE_ID = "golden-gift-li-culture-v1"
_REPO_ROOT = Path(__file__).resolve().parents[3]
_LAB_ROOT = _REPO_ROOT / "ai-ip-evals" / "lab"
_SOURCE_SCHEMA = _LAB_ROOT / "schemas" / "source-import.schema.json"
_BUNDLE_SCHEMA = _LAB_ROOT / "schemas" / "case-bundle.schema.json"
_EXPECTED_SOURCES = {
    "v6-a113-community-acceptance": (
        "A113",
        "douyin-community-mcp-a113-2026-08-20.json",
        ["contentEvidence", "referenceEvidence"],
    ),
    "v6-a115-known-failure": (
        "A115",
        "a115-golden-gift-real-e2e-2026-08-20.json",
        ["outcomeEvidence", "referenceEvidence"],
    ),
    "v6-a116-pollution-diagnostic": (
        "A116",
        "a116-vertical-incubation-skill-live-2026-08-20.json",
        ["outcomeEvidence", "referenceEvidence"],
    ),
}


class CaseFactoryError(ValueError):
    pass


def _load_contract(path: Path, schema: Path, description: str):
    try:
        payload = Path(path).read_bytes()
        value = _load_exact_json_bytes(payload)
        validate_contract(value, schema)
    except (LabContractError, OSError) as error:
        raise CaseFactoryError(f"{description} validation failed") from error
    if type(value) is not dict:
        raise CaseFactoryError(f"{description} must be a JSON object")
    return value, payload


def _validate_manifest_identity(manifest):
    if manifest.get("caseFamilyId") != CASE_ID:
        raise CaseFactoryError("manifest case family is not the frozen case")
    entries = manifest["sources"]
    if len(entries) != len(_EXPECTED_SOURCES):
        raise CaseFactoryError("manifest must bind exactly three sources")
    seen = set()
    for entry in entries:
        source_id = entry["sourceId"]
        if source_id in seen or source_id not in _EXPECTED_SOURCES:
            raise CaseFactoryError("manifest contains an unexpected source identity")
        seen.add(source_id)
        _, relative_path, roles = _EXPECTED_SOURCES[source_id]
        if entry["relativePath"] != relative_path or entry["packetRoles"] != roles:
            raise CaseFactoryError("manifest source binding differs from frozen case")
    if seen != set(_EXPECTED_SOURCES):
        raise CaseFactoryError("manifest is missing a frozen source")


def _parse_compiled_at(value):
    if type(value) is not str:
        raise CaseFactoryError("compiledAt must be an RFC 3339 string")
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as error:
        raise CaseFactoryError("compiledAt must be an RFC 3339 date-time") from error
    if parsed.tzinfo is None:
        raise CaseFactoryError("compiledAt must include a UTC offset")
    return parsed.astimezone(UTC)


def _source_time(value, audit):
    if type(value) is not str:
        raise CaseFactoryError(f"{audit} source record time is missing")
    try:
        if len(value) == 10:
            source_date = date.fromisoformat(value)
            return datetime.combine(source_date, time.max, UTC), True
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as error:
        raise CaseFactoryError(f"{audit} source record time is invalid") from error
    if parsed.tzinfo is None:
        raise CaseFactoryError(f"{audit} source record time lacks a UTC offset")
    return parsed.astimezone(UTC), False


def _validate_source_root(source_root, expected_names):
    root = Path(source_root)
    try:
        metadata = root.lstat()
        if stat.S_ISLNK(metadata.st_mode) or not stat.S_ISDIR(metadata.st_mode):
            raise CaseFactoryError("source root must be a non-symlink directory")
        with os.scandir(root) as entries:
            actual_names = {entry.name for entry in entries}
    except OSError as error:
        raise CaseFactoryError("source root is unavailable") from error
    if actual_names != expected_names:
        raise CaseFactoryError("source root has missing or extra entries")


def _read_source(source_root, entry):
    path = Path(source_root) / entry["relativePath"]
    flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_CLOEXEC", 0)
    descriptor = None
    try:
        before = path.lstat()
        if stat.S_ISLNK(before.st_mode) or not stat.S_ISREG(before.st_mode):
            raise CaseFactoryError("manifest source must be a regular non-symlink file")
        descriptor = os.open(path, flags)
        opened = os.fstat(descriptor)
        if not stat.S_ISREG(opened.st_mode) or (opened.st_dev, opened.st_ino) != (
            before.st_dev,
            before.st_ino,
        ):
            raise CaseFactoryError("manifest source changed while opening")
        chunks = []
        while chunk := os.read(descriptor, 1024 * 1024):
            chunks.append(chunk)
        after = os.fstat(descriptor)
        if (opened.st_dev, opened.st_ino, opened.st_size) != (
            after.st_dev,
            after.st_ino,
            after.st_size,
        ):
            raise CaseFactoryError("manifest source changed while reading")
    except CaseFactoryError:
        raise
    except OSError as error:
        raise CaseFactoryError("cannot safely read manifest source") from error
    finally:
        if descriptor is not None:
            os.close(descriptor)
    payload = b"".join(chunks)
    if hashlib.sha256(payload).hexdigest() != entry["sha256"]:
        raise CaseFactoryError("manifest source SHA-256 mismatch")
    try:
        value = _load_exact_json_bytes(payload)
    except LabContractError as error:
        raise CaseFactoryError("manifest source is not exact JSON") from error
    if type(value) is not dict:
        raise CaseFactoryError("manifest source must be a JSON object")
    return value


def _object(value, key, audit):
    item = value.get(key)
    if type(item) is not dict:
        raise CaseFactoryError(f"{audit} {key} must be an object")
    return item


def _string(value, key, audit):
    item = value.get(key)
    if type(item) is not str or not item:
        raise CaseFactoryError(f"{audit} {key} must be a non-empty string")
    return item


def _validate_source_identity(audit, value):
    identity_key = "audit_id" if audit == "A113" else "audit"
    if value.get(identity_key) != audit:
        raise CaseFactoryError(f"expected {audit} source identity")
    if audit == "A113":
        credentials = _object(value, "credentials", audit)
        if credentials.get("credential_values_recorded") is not False:
            raise CaseFactoryError("A113 records credential values")
    elif value.get("secrets_included") is not False:
        raise CaseFactoryError(f"{audit} includes recorded secrets")


def _validate_source_times(values, compiled_time):
    day_precision = False
    for audit, value in values.items():
        time_key = "recorded_at" if audit == "A113" else "date"
        recorded, date_only = _source_time(value.get(time_key), audit)
        if recorded > compiled_time:
            raise CaseFactoryError("compiledAt precedes a source record")
        if audit == "A113":
            day_precision = date_only
    return day_precision


def _project_packets(blueprint, values, compiled_at, a113_day_precision):
    a113, a115, a116 = values["A113"], values["A115"], values["A116"]
    provider = _object(a113, "provider", "A113")
    acceptance = _object(a113, "deerflow_acceptance", "A113")
    result_count = acceptance.get("results")
    payload_size = acceptance.get("tool_payload_bytes")
    if (
        type(result_count) is not int
        or result_count < 0
        or type(payload_size) is not int
        or payload_size < 0
    ):
        raise CaseFactoryError("A113 result or payload evidence is invalid")
    limitations = [_string(a113, "production_status", "A113")]
    if a113_day_precision:
        limitations.append(
            f"A113 recorded_at has day precision; ordering treats "
            f"{a113['recorded_at']} as inclusive end-of-day UTC without inventing "
            "second-level precision."
        )
    content = {
        "schemaVersion": 1,
        "objectKind": "ContentPacket",
        "caseId": CASE_ID,
        "predictionTime": compiled_at,
        "mission": blueprint["mission"],
        "businessSubject": blueprint["businessSubject"],
        "knownEvidence": {
            "sourceId": "v6-a113-community-acceptance",
            "providerRevision": _string(provider, "revision", "A113"),
            "localExperimentLicenseStatus": _string(provider, "license_status", "A113"),
            "query": _string(acceptance, "query", "A113"),
            "resultCount": result_count,
            "payloadSizeBytes": payload_size,
            "limitations": limitations,
        },
        "unknowns": blueprint["knownUnknowns"],
        "requestedOutput": blueprint["taskLevels"],
        "sourceRefs": ["v6-a113-community-acceptance"],
    }
    a115_semantic = _object(a115, "semantic_result", "A115")
    a115_defect = _object(a115, "post_run_defect", "A115")
    a116_full_run = _object(a116, "accepted_full_run", "A116")
    retained = a116.get("failed_runs_retained")
    if type(retained) is not list:
        raise CaseFactoryError("A116 failed_runs_retained must be an array")
    known_failures = [
        f"A115: {_string(a115_semantic, 'failure', 'A115')}",
        f"A115: {_string(a115_defect, 'symptom', 'A115')}",
        f"A116 diagnostic: {_string(a116_full_run, 'remaining_failure', 'A116')}",
    ]
    for failure in retained:
        if type(failure) is not dict:
            raise CaseFactoryError("A116 retained failure must be an object")
        known_failures.append(f"A116 diagnostic: {_string(failure, 'failure', 'A116')}")
    outcome = {
        "schemaVersion": 1,
        "objectKind": "OutcomePacket",
        "caseId": CASE_ID,
        "revealAfter": compiled_at,
        "observations": [
            f"A115 status: {_string(a115, 'status', 'A115')}",
            f"A116 diagnostic status: {_string(a116, 'status', 'A116')}",
        ],
        "knownFailures": known_failures,
        "sourceRefs": [
            "v6-a115-known-failure",
            "v6-a116-pollution-diagnostic",
        ],
    }
    reference = {
        "schemaVersion": 1,
        "objectKind": "ReferenceDossier",
        "caseId": CASE_ID,
        "directionFamilies": blueprint["directionFamilies"],
        "counterexamples": known_failures,
        "nonCopyBoundaries": [
            "The three direction families are conditional options, not mandatory roots.",
            "A115 failures are counterexamples, not answer templates.",
            "A116 is diagnostic evidence and must not become a gold content root.",
        ],
        "disputes": [
            "A113 local use remains bounded by its recorded provider license status."
        ],
        "sourceRefs": list(_EXPECTED_SOURCES),
    }
    return content, outcome, reference


def _validate_outputs(values):
    try:
        for value in values:
            validate_contract(value, _BUNDLE_SCHEMA)
    except LabContractError as error:
        raise CaseFactoryError("compiled case contract validation failed") from error


def _write_outputs(private_root, content, outcome, reference, receipt):
    base = f"cases/{CASE_ID}"
    try:
        private_root.create_dir("cases")
        private_root.create_dir(base)
        for directory in ("content", "outcome", "reference", "coordinator"):
            private_root.create_dir(f"{base}/{directory}")
        private_root.write_new_json(f"{base}/content/content-packet.json", content)
        private_root.write_new_json(f"{base}/outcome/outcome-packet.json", outcome)
        private_root.write_new_json(
            f"{base}/reference/reference-dossier.json", reference
        )
        private_root.write_new_json(
            f"{base}/coordinator/case-compilation-receipt.json", receipt
        )
    except (LabContractError, OSError) as error:
        raise CaseFactoryError("cannot write compiled case") from error


def compile_golden_gift_case(
    source_root: Path,
    blueprint_path: Path,
    source_manifest_path: Path,
    private_root: PrivateRoot,
    compiled_at: str,
) -> CaseCompilationReceipt:
    manifest, manifest_payload = _load_contract(
        source_manifest_path, _SOURCE_SCHEMA, "manifest"
    )
    blueprint, _ = _load_contract(blueprint_path, _BUNDLE_SCHEMA, "blueprint")
    _validate_manifest_identity(manifest)
    if blueprint.get("caseFamilyId") != CASE_ID:
        raise CaseFactoryError("blueprint case family is not the frozen case")
    compiled_time = _parse_compiled_at(compiled_at)

    entries = {entry["sourceId"]: entry for entry in manifest["sources"]}
    _validate_source_root(
        source_root, {entry["relativePath"] for entry in manifest["sources"]}
    )
    sources = {}
    for source_id, (audit, _, _) in _EXPECTED_SOURCES.items():
        value = _read_source(source_root, entries[source_id])
        _validate_source_identity(audit, value)
        sources[audit] = value
    day_precision = _validate_source_times(sources, compiled_time)

    content, outcome, reference = _project_packets(
        blueprint, sources, compiled_at, day_precision
    )
    receipt: CaseCompilationReceipt = {
        "schemaVersion": 1,
        "objectKind": "CaseCompilationReceipt",
        "caseId": CASE_ID,
        "contentPacketSha256": sha256_json(content),
        "outcomePacketSha256": sha256_json(outcome),
        "referenceDossierSha256": sha256_json(reference),
        "sourceManifestSha256": hashlib.sha256(manifest_payload).hexdigest(),
    }
    _validate_outputs((content, outcome, reference, receipt))
    _write_outputs(private_root, content, outcome, reference, receipt)
    return receipt
