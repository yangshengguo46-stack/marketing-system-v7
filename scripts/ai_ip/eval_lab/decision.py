"""Blind-only review validation and immutable diagnostic statistics sealing."""

import re
from datetime import datetime, timezone
from pathlib import Path

if __package__:
    from .contracts import (
        BlindStatistics,
        LabContractError,
        load_exact_json,
        sha256_json,
        validate_contract,
    )
    from .private_fs import PrivateRoot
else:
    from contracts import (
        BlindStatistics,
        LabContractError,
        load_exact_json,
        sha256_json,
        validate_contract,
    )
    from private_fs import PrivateRoot


class DecisionError(ValueError):
    """Raised when blind review authority or content is invalid."""


class ArbitrationRequiredError(DecisionError):
    """Raised when valid base judgments require an absent arbitration pair."""

    def __init__(self, preference_counts: dict[str, int]):
        super().__init__("a designated arbitrator primary/swap pair is required")
        self.preference_counts = preference_counts


_LAB_ROOT = Path(__file__).resolve().parents[3] / "ai-ip-evals" / "lab"
_BLIND_SCHEMA = _LAB_ROOT / "schemas" / "blind-review.schema.json"
_BATCH_SCHEMA = _LAB_ROOT / "schemas" / "batch.schema.json"
_REVIEWER_SCHEMA = _LAB_ROOT / "schemas" / "reviewer.schema.json"
_CASE_SCHEMA = _LAB_ROOT / "schemas" / "case-bundle.schema.json"
_REQUIRED_DOMAINS = {"businessIpJudgment", "evidenceIntegrity"}
_COMPONENT = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}")
_RFC3339 = re.compile(
    r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?"
    r"(?:Z|[+-](?:0\d|1\d|2[0-3]):[0-5]\d)"
)
_SEVERE_ORDER = (
    "fabricatedExistingExperience",
    "wrongBusinessSubject",
    "wrongDesiredAction",
    "singlePathPresentedAsProvenTruth",
    "notActuallyUsable",
    "rightsOrPrivacyViolation",
)


def _time(value: object, label: str) -> datetime:
    if type(value) is not str or _RFC3339.fullmatch(value) is None:
        raise DecisionError(f"{label} must be a strict RFC3339 timestamp")
    try:
        parsed = datetime.fromisoformat(value[:-1] + "+00:00" if value.endswith("Z") else value)
    except ValueError as error:
        raise DecisionError(f"{label} must be a strict RFC3339 timestamp") from error
    return parsed.astimezone(timezone.utc)


def _component(value: object, label: str) -> str:
    if type(value) is not str or _COMPONENT.fullmatch(value) is None:
        raise DecisionError(f"invalid {label}")
    return value


def _validate(value: object, schema: Path, label: str) -> dict[str, object]:
    try:
        validate_contract(value, schema)
    except LabContractError as error:
        raise DecisionError(f"invalid {label}") from error
    if type(value) is not dict:
        raise DecisionError(f"invalid {label}")
    return value


def _receipt_location(private_root: PrivateRoot, path: Path) -> tuple[str, str]:
    candidate = Path(path)
    if not candidate.is_absolute():
        raise DecisionError("blind-pack receipt path must be absolute")
    try:
        relative = candidate.relative_to(private_root.path)
    except ValueError as error:
        raise DecisionError(
            "blind-pack receipt is outside the trusted private root"
        ) from error
    parts = relative.parts
    if (
        len(parts) != 3
        or parts[0] != "batches"
        or parts[2] != "blind-pack-receipt.json"
    ):
        raise DecisionError("blind-pack receipt path is not authoritative")
    return relative.as_posix(), _component(parts[1], "batchId")


def _external_path(
    private_root: PrivateRoot, batch_id: str, path: Path, label: str
) -> Path:
    candidate = Path(path)
    if not candidate.is_absolute():
        raise DecisionError(f"{label} path must be absolute")
    try:
        candidate.relative_to(private_root.path)
    except ValueError:
        pass
    else:
        raise DecisionError(f"{label} path must be outside the private root")
    try:
        resolved = candidate.resolve(strict=True)
        resolved.relative_to(private_root.path)
    except ValueError:
        pass
    except OSError as error:
        raise DecisionError(f"cannot authorize {label} path") from error
    else:
        raise DecisionError(f"{label} path resolves inside the private root")
    try:
        candidate_metadata = resolved.stat()
        arm_key_metadata = (
            private_root.path
            / f"batches/{batch_id}/coordinator/arm-key.json"
        ).stat()
    except OSError as error:
        raise DecisionError(f"cannot authorize {label} path") from error
    if (candidate_metadata.st_dev, candidate_metadata.st_ino) == (
        arm_key_metadata.st_dev,
        arm_key_metadata.st_ino,
    ):
        raise DecisionError(f"{label} aliases the private arm key")
    return resolved


def _external(
    private_root: PrivateRoot,
    batch_id: str,
    path: Path,
    schema: Path,
    label: str,
) -> dict[str, object]:
    authorized = _external_path(private_root, batch_id, path, label)
    try:
        return _validate(load_exact_json(authorized), schema, label)
    except (LabContractError, OSError) as error:
        raise DecisionError(f"cannot load {label}") from error


def _qualification(
    private_root: PrivateRoot, batch_id: str, path: Path, sealed: datetime
) -> dict[str, object]:
    receipt = _external(
        private_root, batch_id, path, _REVIEWER_SCHEMA, "qualification receipt"
    )
    if (
        receipt.get("objectKind") != "QualificationReceipt"
        or receipt.get("diagnosticOnly") is not True
        or receipt.get("status") != "qualified"
    ):
        raise DecisionError("reviewer is not actively qualified")
    domains = receipt.get("qualifiedDomains")
    if type(domains) is not list or not _REQUIRED_DOMAINS.issubset(set(domains)):
        raise DecisionError("reviewer lacks required qualification domains")
    if sealed >= _time(receipt.get("expiresAt"), "expiresAt"):
        raise DecisionError("reviewer qualification is expired")
    _component(receipt.get("reviewerId"), "reviewerId")
    return receipt


def _private_json(
    private_root: PrivateRoot, relative: str, schema: Path, label: str
) -> dict[str, object]:
    try:
        return _validate(private_root.read_json(relative), schema, label)
    except (LabContractError, OSError) as error:
        raise DecisionError(f"cannot load {label}") from error


def _load_pack(
    private_root: PrivateRoot,
    relative: str,
    batch_id: str,
    reviewer_ids: list[str],
) -> tuple[dict[str, object], dict[str, dict[str, object]], dict[str, set[str]]]:
    try:
        receipt = _validate(private_root.read_json(relative), _BLIND_SCHEMA, "blind-pack receipt")
    except (LabContractError, OSError) as error:
        raise DecisionError("cannot load blind-pack receipt") from error
    if receipt.get("objectKind") != "BlindPackReceipt":
        raise DecisionError("wrong blind-pack receipt object kind")
    if receipt.get("batchId") != batch_id:
        raise DecisionError("blind-pack receipt path/batch binding mismatch")
    base = f"batches/{batch_id}"
    manifest = _private_json(
        private_root, f"{base}/coordinator/batch-manifest.json", _BATCH_SCHEMA, "batch manifest"
    )
    if (
        manifest.get("objectKind") != "EvaluationBatch"
        or manifest.get("batchId") != batch_id
        or manifest.get("diagnosticOnly") is not True
        or sha256_json(manifest) != receipt.get("batchManifestSha256")
        or manifest.get("reviewerIds") != reviewer_ids
    ):
        raise DecisionError("blind-pack manifest binding mismatch")

    count = len(reviewer_ids)
    assignments: dict[str, dict[str, object]] = {}
    evidence: dict[str, set[str]] = {}
    ordered: list[dict[str, object]] = []
    for index, reviewer_id in enumerate(reviewer_ids):
        primary_id = None
        for role, slot in (("primary", index + 1), ("swap", count + index + 1)):
            label = f"{reviewer_id}-{role}"
            root = f"{base}/reviewer/{reviewer_id}/{label}"
            assignment = _private_json(
                private_root, f"{root}/assignment.json", _BLIND_SCHEMA, "blind assignment"
            )
            expected_release = "immediate" if index < 2 else "arbitrationRequired"
            if (
                assignment.get("objectKind") != "BlindAssignment"
                or assignment.get("batchId") != batch_id
                or assignment.get("reviewerId") != reviewer_id
                or assignment.get("sequenceSlot") != slot
                or assignment.get("releaseCondition") != expected_release
                or assignment.get("swapOf") != (None if role == "primary" else primary_id)
            ):
                raise DecisionError("blind assignment authority mismatch")
            assignment_id = _component(assignment.get("assignmentId"), "assignmentId")
            if assignment_id in assignments:
                raise DecisionError("duplicate blind assignment")
            packet = _private_json(
                private_root, f"{root}/content-packet.json", _CASE_SCHEMA, "content packet"
            )
            refs = packet.get("sourceRefs")
            if (
                packet.get("objectKind") != "ContentPacket"
                or sha256_json(packet) != assignment.get("contentPacketSha256")
                or type(refs) is not list
            ):
                raise DecisionError("assignment content binding mismatch")
            assignments[assignment_id] = assignment
            evidence[assignment_id] = set(refs)
            ordered.append(assignment)
            if role == "primary":
                primary_id = assignment_id
        primary, swap = ordered[index * 2 : index * 2 + 2]
        if (
            primary["position"] == swap["position"]
            or primary["armAOutputSha256"] != swap["armBOutputSha256"]
            or primary["armBOutputSha256"] != swap["armAOutputSha256"]
        ):
            raise DecisionError("primary/swap output binding mismatch")
    by_slot = sorted(ordered, key=lambda item: item["sequenceSlot"])
    hashes = [sha256_json(value) for value in by_slot]
    outputs = {
        value[key]
        for value in ordered
        for key in ("armAOutputSha256", "armBOutputSha256")
    }
    if (
        hashes != receipt.get("assignmentSha256s")
        or type(receipt.get("mappingSha256s")) is not list
        or len(receipt["mappingSha256s"]) != count * 2
        or len(outputs) != 2
        or set(manifest.get("armOutputSha256s", [])) != outputs
    ):
        raise DecisionError("blind-pack assignment hash binding mismatch")
    return receipt, assignments, evidence


def _submission_value(
    private_root: PrivateRoot,
    batch_id: str,
    path: Path,
    assignments: dict[str, dict[str, object]],
    evidence: dict[str, set[str]],
    sealed: datetime,
) -> tuple[str, dict[str, object]]:
    value = _external(
        private_root, batch_id, path, _BLIND_SCHEMA, "review submission"
    )
    if value.get("objectKind") != "ReviewSubmission":
        raise DecisionError("wrong review submission object kind")
    assignment_id = value.get("assignmentId")
    assignment = assignments.get(assignment_id) if type(assignment_id) is str else None
    if assignment is None or value.get("reviewerId") != assignment.get("reviewerId"):
        raise DecisionError("submission reviewer/assignment binding mismatch")
    if not set(value["evidenceRefs"]).issubset(evidence[assignment_id]):
        raise DecisionError("submission cites evidence outside the content packet")
    if _time(value.get("submittedAt"), "submittedAt") > sealed:
        raise DecisionError("submission occurs after sealing")
    eligibility, preference = value["eligibility"], value["preference"]
    required = {"aOnly": "A", "bOnly": "B", "neither": "abstain"}.get(eligibility)
    if required is not None and preference != required:
        raise DecisionError("submission eligibility/preference mismatch")
    return assignment_id, value


def _by_output(value: dict[str, object], assignment: dict[str, object], field: str) -> dict[str, object]:
    return {
        assignment[f"arm{arm}OutputSha256"]: value[field][arm]
        for arm in ("A", "B")
    }


def _pair(
    primary: tuple[dict[str, object], dict[str, object]],
    swap: tuple[dict[str, object], dict[str, object]],
    output_labels: dict[str, str],
) -> tuple[str, bool, dict[str, list[str]]]:
    judgments = []
    eligible_sets = []
    mapped = []
    for assignment, submission in (primary, swap):
        dimensions = _by_output(submission, assignment, "dimensionsByArm")
        severe = _by_output(submission, assignment, "severeFlagsByArm")
        mapped.append((dimensions, severe))
        eligibility = submission["eligibility"]
        arms = {"both": ("A", "B"), "aOnly": ("A",), "bOnly": ("B",), "neither": ()}[eligibility]
        eligible_sets.append({assignment[f"arm{arm}OutputSha256"] for arm in arms})
        preference = submission["preference"]
        judgments.append(
            assignment[f"arm{preference}OutputSha256"] if preference in ("A", "B") else preference
        )
    if mapped[0] != mapped[1]:
        raise DecisionError("primary/swap dimension or severe binding mismatch")
    severe = {output: list(flags) for output, flags in mapped[0][1].items()}
    consistent = judgments[0] == judgments[1] and eligible_sets[0] == eligible_sets[1]
    judgment = judgments[0] if consistent else "nearTie"
    return output_labels.get(judgment, judgment), consistent, severe


def _counts(preferences: list[str]) -> dict[str, int]:
    return {name: preferences.count(name) for name in ("A", "B", "nearTie", "abstain")}


def seal_blind_statistics(
    *,
    private_root: PrivateRoot,
    blind_pack_receipt_path: Path,
    base_qualification_receipt_paths: tuple[Path, Path],
    arbitrator_qualification_receipt_path: Path | None,
    submission_paths: tuple[Path, ...],
    sealed_at: str,
) -> BlindStatistics:
    """Validate opaque paired judgments and create one final blind-statistics seal."""
    sealed = _time(sealed_at, "sealedAt")
    if type(base_qualification_receipt_paths) is not tuple or len(base_qualification_receipt_paths) != 2:
        raise DecisionError("exactly two base qualification receipts are required")
    if type(submission_paths) is not tuple:
        raise DecisionError("submission_paths must be a tuple")
    receipt_relative, batch_id = _receipt_location(
        private_root, blind_pack_receipt_path
    )
    base_receipts = [
        _qualification(private_root, batch_id, path, sealed)
        for path in base_qualification_receipt_paths
    ]
    arbitrator = (
        _qualification(
            private_root,
            batch_id,
            arbitrator_qualification_receipt_path,
            sealed,
        )
        if arbitrator_qualification_receipt_path is not None
        else None
    )
    reviewer_ids = [receipt["reviewerId"] for receipt in base_receipts]
    if arbitrator is not None:
        reviewer_ids.append(arbitrator["reviewerId"])
    if len(reviewer_ids) != len(set(reviewer_ids)):
        raise DecisionError("reviewer identities must be distinct")
    receipt, assignments, evidence = _load_pack(
        private_root, receipt_relative, batch_id, reviewer_ids
    )
    submissions: dict[str, dict[str, object]] = {}
    for path in submission_paths:
        assignment_id, value = _submission_value(
            private_root, batch_id, path, assignments, evidence, sealed
        )
        if assignment_id in submissions:
            raise DecisionError("duplicate review assignment")
        submissions[assignment_id] = value

    first = assignments[next(iter(assignments))]
    output_labels = {
        first["armAOutputSha256"]: "A",
        first["armBOutputSha256"]: "B",
    }
    preferences: list[str] = []
    severe_sets = {output: set() for output in output_labels}
    consistent_count = 0

    def consume(reviewer_index: int) -> tuple[str, bool]:
        reviewer_id = reviewer_ids[reviewer_index]
        values = []
        for role in ("primary", "swap"):
            label = f"{reviewer_id}-{role}"
            assignment = next(
                value
                for value in assignments.values()
                if value["reviewerId"] == reviewer_id
                and value["sequenceSlot"] == (reviewer_index + 1 if role == "primary" else len(reviewer_ids) + reviewer_index + 1)
            )
            submission = submissions.get(assignment["assignmentId"])
            if submission is None:
                raise KeyError(label)
            values.append((assignment, submission))
        preference, consistent, severe = _pair(values[0], values[1], output_labels)
        for output, flags in severe.items():
            severe_sets[output].update(flags)
        return preference, consistent

    try:
        for index in range(2):
            preference, consistent = consume(index)
            preferences.append(preference)
            consistent_count += int(consistent)
    except KeyError as error:
        raise DecisionError("exactly four base primary/swap submissions are required") from error

    arbitration_required = (
        preferences[0] != preferences[1]
        or any(value in ("nearTie", "abstain") for value in preferences)
        or any(severe_sets.values())
    )
    arbitration_completed = False
    if arbitration_required:
        if arbitrator is None:
            raise ArbitrationRequiredError(_counts(preferences))
        try:
            preference, consistent = consume(2)
        except KeyError as error:
            raise ArbitrationRequiredError(_counts(preferences)) from error
        if not consistent or preference not in ("A", "B"):
            raise DecisionError("arbitrator primary/swap judgments must resolve to one output")
        preferences.append(preference)
        consistent_count += 1
        arbitration_completed = True
    else:
        base_assignment_ids = {
            assignment["assignmentId"]
            for assignment in assignments.values()
            if assignment["reviewerId"] in reviewer_ids[:2]
        }
        if set(submissions) != base_assignment_ids:
            raise DecisionError("arbitrator submissions cannot be counted without arbitration")

    expected_submission_count = 6 if arbitration_completed else 4
    if len(submissions) != expected_submission_count:
        raise DecisionError("review submission set is not exact")
    statistics: BlindStatistics = {
        "schemaVersion": 1,
        "objectKind": "BlindStatistics",
        "batchId": receipt["batchId"],
        "reviewCount": expected_submission_count,
        "qualifiedReviewerCount": len(preferences),
        "swapConsistentCount": consistent_count,
        "preferenceCounts": _counts(preferences),
        "severeByOutputSha256": {
            output: [flag for flag in _SEVERE_ORDER if flag in severe_sets[output]]
            for output in output_labels
        },
        "arbitrationRequired": arbitration_required,
        "arbitrationCompleted": arbitration_completed,
        "sealedAt": sealed_at,
    }
    _validate(statistics, _BATCH_SCHEMA, "blind statistics")
    relative = f"batches/{receipt['batchId']}/coordinator/blind-statistics.json"
    try:
        private_root.write_new_json(relative, statistics)
    except (LabContractError, OSError) as error:
        raise DecisionError("cannot create blind statistics") from error
    return statistics
