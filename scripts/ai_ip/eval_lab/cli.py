"""Narrow, provider-free command line for the diagnostic evaluation lab."""

import argparse
import sys
from pathlib import Path

if __package__:
    from .blind_controller import BlindControllerError, prepare_blind_batch
    from .case_factory import CaseFactoryError, compile_golden_gift_case
    from .contracts import LabContractError, canonical_json_bytes, load_exact_json
    from .decision import (
        ArbitrationRequiredError,
        DecisionError,
        seal_blind_statistics,
    )
    from .private_fs import PrivateRoot
    from .reviewer_academy import ReviewerAcademyError, evaluate_calibration
    from .unlock import UnlockError, unlock_fixture_pilot
else:
    from blind_controller import BlindControllerError, prepare_blind_batch
    from case_factory import CaseFactoryError, compile_golden_gift_case
    from contracts import LabContractError, canonical_json_bytes, load_exact_json
    from decision import (
        ArbitrationRequiredError,
        DecisionError,
        seal_blind_statistics,
    )
    from private_fs import PrivateRoot
    from reviewer_academy import ReviewerAcademyError, evaluate_calibration
    from unlock import UnlockError, unlock_fixture_pilot


class _ArgumentError(ValueError):
    pass


class _SafeParser(argparse.ArgumentParser):
    def error(self, message: str) -> None:
        del message
        raise _ArgumentError("invalid arguments")


_DOMAIN_ERRORS = (
    LabContractError,
    CaseFactoryError,
    ReviewerAcademyError,
    BlindControllerError,
    DecisionError,
    ArbitrationRequiredError,
    UnlockError,
)


def _parser() -> _SafeParser:
    parser = _SafeParser(add_help=False, allow_abbrev=False)
    commands = parser.add_subparsers(dest="command", required=True)

    compile_parser = commands.add_parser(
        "compile-golden-gift", add_help=False, allow_abbrev=False
    )
    _required(
        compile_parser,
        "source-root",
        "source-manifest",
        "blueprint",
        "private-root",
        "compiled-at",
    )

    qualify_parser = commands.add_parser(
        "qualify-reviewer", add_help=False, allow_abbrev=False
    )
    _required(
        qualify_parser,
        "profile",
        "attempt",
        "policy",
        "evaluated-at",
        "output-root",
    )

    prepare_parser = commands.add_parser(
        "prepare-blind-batch", add_help=False, allow_abbrev=False
    )
    _required(
        prepare_parser,
        "private-root",
        "case-receipt",
        "stock-answer",
        "modified-answer",
        "rubric",
        "batch-id",
        "analysis-frozen-at",
    )
    prepare_parser.add_argument(
        "--base-qualification-receipt", action="append", required=True
    )
    prepare_parser.add_argument(
        "--arbitrator-qualification-receipt", action="append"
    )

    seal_parser = commands.add_parser(
        "seal-blind-statistics", add_help=False, allow_abbrev=False
    )
    _required(seal_parser, "private-root", "blind-pack-receipt", "sealed-at")
    seal_parser.add_argument(
        "--base-qualification-receipt", action="append", required=True
    )
    seal_parser.add_argument(
        "--arbitrator-qualification-receipt", action="append"
    )
    seal_parser.add_argument("--submission", action="append", required=True)

    unlock_parser = commands.add_parser(
        "unlock-fixture-pilot", add_help=False, allow_abbrev=False
    )
    _required(
        unlock_parser,
        "private-root",
        "blind-statistics",
        "arm-key",
        "case-receipt",
        "outcome-packet",
        "unlocked-at",
    )
    return parser


def _required(parser: argparse.ArgumentParser, *names: str) -> None:
    for name in names:
        parser.add_argument(f"--{name}", required=True)


def _path(value: str) -> Path:
    return Path(value).resolve(strict=False)


def _optional_path(values: list[str] | None) -> Path | None:
    return None if values is None else _path(values[0])


def _load_input(path: Path) -> object:
    try:
        return load_exact_json(path)
    except OSError as error:
        raise LabContractError("input JSON is unavailable") from error


def _check_cardinality(arguments: argparse.Namespace) -> None:
    command = arguments.command
    if command in ("prepare-blind-batch", "seal-blind-statistics"):
        if len(arguments.base_qualification_receipt) != 2:
            raise _ArgumentError("invalid arguments")
        arbitrators = arguments.arbitrator_qualification_receipt or []
        if len(arbitrators) > 1:
            raise _ArgumentError("invalid arguments")
    if command == "seal-blind-statistics" and len(arguments.submission) not in (
        4,
        6,
    ):
        raise _ArgumentError("invalid arguments")


def _compile(arguments: argparse.Namespace) -> dict[str, object]:
    private_root = PrivateRoot.create_new(_path(arguments.private_root))
    receipt = compile_golden_gift_case(
        _path(arguments.source_root),
        _path(arguments.blueprint),
        _path(arguments.source_manifest),
        private_root,
        arguments.compiled_at,
    )
    return {"objectKind": receipt["objectKind"], "caseId": receipt["caseId"]}


def _qualify(arguments: argparse.Namespace) -> dict[str, object]:
    receipt = evaluate_calibration(
        _load_input(_path(arguments.profile)),
        _load_input(_path(arguments.attempt)),
        _load_input(_path(arguments.policy)),
        arguments.evaluated_at,
    )
    output_root = PrivateRoot.create_new(_path(arguments.output_root))
    output_root.write_new_json("qualification-receipt.json", receipt)
    return {
        "objectKind": receipt["objectKind"],
        "status": receipt["status"],
        "diagnosticOnly": receipt["diagnosticOnly"],
    }


def _prepare(arguments: argparse.Namespace) -> dict[str, object]:
    receipt = prepare_blind_batch(
        private_root=PrivateRoot.open_existing(_path(arguments.private_root)),
        case_receipt_path=_path(arguments.case_receipt),
        stock_answer_path=_path(arguments.stock_answer),
        modified_answer_path=_path(arguments.modified_answer),
        rubric_path=_path(arguments.rubric),
        base_qualification_receipt_paths=tuple(
            _path(value) for value in arguments.base_qualification_receipt
        ),
        arbitrator_qualification_receipt_path=_optional_path(
            arguments.arbitrator_qualification_receipt
        ),
        batch_id=arguments.batch_id,
        analysis_frozen_at=arguments.analysis_frozen_at,
    )
    return {"objectKind": receipt["objectKind"], "batchId": receipt["batchId"]}


def _seal(arguments: argparse.Namespace) -> dict[str, object]:
    statistics = seal_blind_statistics(
        private_root=PrivateRoot.open_existing(_path(arguments.private_root)),
        blind_pack_receipt_path=_path(arguments.blind_pack_receipt),
        base_qualification_receipt_paths=tuple(
            _path(value) for value in arguments.base_qualification_receipt
        ),
        arbitrator_qualification_receipt_path=_optional_path(
            arguments.arbitrator_qualification_receipt
        ),
        submission_paths=tuple(_path(value) for value in arguments.submission),
        sealed_at=arguments.sealed_at,
    )
    return {
        "objectKind": statistics["objectKind"],
        "batchId": statistics["batchId"],
        "arbitrationRequired": statistics["arbitrationRequired"],
        "arbitrationCompleted": statistics["arbitrationCompleted"],
    }


def _unlock(arguments: argparse.Namespace) -> dict[str, object]:
    decision = unlock_fixture_pilot(
        private_root=PrivateRoot.open_existing(_path(arguments.private_root)),
        blind_statistics_path=_path(arguments.blind_statistics),
        arm_key_path=_path(arguments.arm_key),
        case_receipt_path=_path(arguments.case_receipt),
        outcome_packet_path=_path(arguments.outcome_packet),
        unlocked_at=arguments.unlocked_at,
    )
    return {
        "batchId": decision["batchId"],
        "diagnosticOnly": decision["diagnosticOnly"],
        "disposition": decision["disposition"],
        "promotionEligible": decision["promotionEligible"],
        "providerMode": decision["providerMode"],
    }


_HANDLERS = {
    "compile-golden-gift": _compile,
    "qualify-reviewer": _qualify,
    "prepare-blind-batch": _prepare,
    "seal-blind-statistics": _seal,
    "unlock-fixture-pilot": _unlock,
}


def main(argv: list[str] | None = None) -> int:
    try:
        arguments = _parser().parse_args(argv)
        _check_cardinality(arguments)
    except _ArgumentError:
        sys.stderr.write("invalid arguments\n")
        return 2
    try:
        projection = _HANDLERS[arguments.command](arguments)
        payload = canonical_json_bytes(projection).decode("utf-8")
    except _DOMAIN_ERRORS:
        sys.stderr.write("lab command failed\n")
        return 1
    sys.stdout.write(payload + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
