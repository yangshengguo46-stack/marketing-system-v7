"""Persistent plan reservation derivation for public controller entry points."""

from pathlib import Path

try:
    from .batch_controller_support import (
        identities,
        pair_seed,
        replication_seeds,
        require_seed,
    )
    from .batch_receipts import assert_private_layout_available
except ImportError:
    from batch_controller_support import (
        identities,
        pair_seed,
        replication_seeds,
        require_seed,
    )
    from batch_receipts import assert_private_layout_available


def reserve_single(
    plan: dict[str, object], private_root: Path, seed: bytes | None, reserve: object
) -> bytes:
    active_seed = pair_seed(require_seed(seed), private_root, plan["planSha256"])
    identity = identities(active_seed)
    assert_private_layout_available(private_root, (identity,))
    reserve(private_root, str(plan["planSha256"]), 1, (identity[0],))
    return active_seed


def reserve_batch(
    plan: dict[str, object], private_root: Path, seed: bytes | None, reserve: object
) -> tuple[bytes, ...]:
    seeds = replication_seeds(plan, private_root, seed)
    batch_identities = tuple(identities(active_seed) for active_seed in seeds)
    assert_private_layout_available(private_root, batch_identities)
    reserve(
        private_root,
        str(plan["planSha256"]),
        len(seeds),
        tuple(value[0] for value in batch_identities),
    )
    return seeds
