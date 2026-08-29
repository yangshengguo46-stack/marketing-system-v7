use crate::cost::CalculatedCost;
use crate::cost::CostCalculationInput;
use crate::cost::calculate_cost;
use crate::cost_contracts::FrozenCostContracts;
use crate::cost_contracts::VerifiedCostInputs;
use crate::model::Usage;
use pretty_assertions::assert_eq;

const RATE_CARD: &[u8] =
    include_bytes!("../tests/fixtures/contracts/06b1/provider-rate-card.canonical.json");
const BILLING_POLICY: &[u8] =
    include_bytes!("../tests/fixtures/contracts/06b1/billing-policy.canonical.json");
const FX_POLICY: &[u8] =
    include_bytes!("../tests/fixtures/contracts/06b1/fx-policy.canonical.json");
const BUDGET: &[u8] =
    include_bytes!("../tests/fixtures/contracts/06b1/provider-budget-evidence.canonical.json");

fn verified_inputs() -> VerifiedCostInputs {
    FrozenCostContracts::load()
        .unwrap()
        .validate_inputs(RATE_CARD, BILLING_POLICY, FX_POLICY, BUDGET)
        .unwrap()
}

fn calculation_input<'a>(
    inputs: &'a VerifiedCostInputs,
    usage: &'a Usage,
) -> CostCalculationInput<'a> {
    CostCalculationInput {
        inputs,
        usage,
        supplier_actual_fen: None,
        approved_per_run_fen: u64::MAX,
        max_total_tokens: u64::MAX,
    }
}

fn zero_usage() -> Usage {
    Usage::default()
}

#[test]
fn cost_arithmetic_charges_each_token_partition_at_its_exact_million_rate() {
    let inputs = verified_inputs();
    let cases = [
        (
            "uncached input",
            Usage {
                total_tokens: 1_000_000,
                input_tokens: 1_000_000,
                ..Usage::default()
            },
            12,
        ),
        (
            "cached input",
            Usage {
                total_tokens: 1_000_000,
                input_tokens: 1_000_000,
                cached_input_tokens: 1_000_000,
                ..Usage::default()
            },
            3,
        ),
        (
            "cache-write input",
            Usage {
                total_tokens: 1_000_000,
                input_tokens: 1_000_000,
                cache_write_input_tokens: 1_000_000,
                ..Usage::default()
            },
            6,
        ),
        (
            "output",
            Usage {
                total_tokens: 1_000_000,
                output_tokens: 1_000_000,
                ..Usage::default()
            },
            24,
        ),
    ];

    for (name, usage, expected_fen) in cases {
        let actual = calculate_cost(&calculation_input(&inputs, &usage))
            .unwrap_or_else(|error| panic!("{name}: {error:#}"));
        assert_eq!(
            actual,
            CalculatedCost {
                estimated_fen: expected_fen,
                supplier_actual_fen: None,
                charged_fen: expected_fen,
                within_ceilings: true,
            },
            "{name}"
        );
    }
}

#[test]
fn cost_arithmetic_rounds_once_after_partitioning_and_does_not_double_charge_reasoning() {
    let inputs = verified_inputs();
    let cases = [
        ("all zero", zero_usage(), 0),
        (
            "one token rounds up",
            Usage {
                total_tokens: 1,
                input_tokens: 1,
                ..Usage::default()
            },
            1,
        ),
        (
            "cached and cache-write subtract from uncached",
            Usage {
                total_tokens: 1_000_000,
                input_tokens: 1_000_000,
                cached_input_tokens: 200_000,
                cache_write_input_tokens: 300_000,
                ..Usage::default()
            },
            9,
        ),
        (
            "reasoning remains within output",
            Usage {
                total_tokens: 1_000_000,
                output_tokens: 1_000_000,
                reasoning_output_tokens: 1_000_000,
                ..Usage::default()
            },
            24,
        ),
    ];

    for (name, usage, expected_fen) in cases {
        let actual = calculate_cost(&calculation_input(&inputs, &usage))
            .unwrap_or_else(|error| panic!("{name}: {error:#}"));
        assert_eq!(actual.estimated_fen, expected_fen, "{name}");
    }
}

#[test]
fn cost_arithmetic_rejects_every_negative_usage_field_before_conversion() {
    let inputs = verified_inputs();
    let fields = [
        "total_tokens",
        "input_tokens",
        "cached_input_tokens",
        "cache_write_input_tokens",
        "output_tokens",
        "reasoning_output_tokens",
    ];

    for (index, field) in fields.into_iter().enumerate() {
        let mut usage = zero_usage();
        match index {
            0 => usage.total_tokens = -1,
            1 => usage.input_tokens = -1,
            2 => usage.cached_input_tokens = -1,
            3 => usage.cache_write_input_tokens = -1,
            4 => usage.output_tokens = -1,
            5 => usage.reasoning_output_tokens = -1,
            _ => unreachable!(),
        }

        let error = calculate_cost(&calculation_input(&inputs, &usage)).unwrap_err();
        assert!(
            error.to_string().contains(&format!("{field} cannot be negative")),
            "{field}: {error:#}"
        );
    }
}

#[test]
fn cost_arithmetic_rejects_contradictory_usage_and_checked_signed_overflow() {
    let inputs = verified_inputs();
    let cases = [
        (
            "total mismatch",
            Usage {
                total_tokens: 2,
                input_tokens: 1,
                ..Usage::default()
            },
            "total_tokens must equal input_tokens plus output_tokens",
        ),
        (
            "reasoning exceeds output",
            Usage {
                total_tokens: 1,
                output_tokens: 1,
                reasoning_output_tokens: 2,
                ..Usage::default()
            },
            "reasoning_output_tokens cannot exceed output_tokens",
        ),
        (
            "cache partitions exceed input",
            Usage {
                total_tokens: 1,
                input_tokens: 1,
                cached_input_tokens: 1,
                cache_write_input_tokens: 1,
                ..Usage::default()
            },
            "cached and cache-write tokens cannot exceed input_tokens",
        ),
        (
            "total addition overflow",
            Usage {
                total_tokens: i64::MAX,
                input_tokens: i64::MAX,
                output_tokens: 1,
                ..Usage::default()
            },
            "input and output token addition overflow",
        ),
        (
            "partition subtraction overflow",
            Usage {
                cached_input_tokens: i64::MAX,
                cache_write_input_tokens: i64::MAX,
                ..Usage::default()
            },
            "input token subtraction overflow",
        ),
    ];

    for (name, usage, expected) in cases {
        let error = calculate_cost(&calculation_input(&inputs, &usage)).unwrap_err();
        assert!(
            error.to_string().contains(expected),
            "{name}: expected {expected:?}, got {error:#}"
        );
    }
}

#[test]
fn cost_arithmetic_checks_multiply_sum_rounding_and_final_conversion_boundaries() {
    let mut inputs = verified_inputs();
    inputs.rate_card.uncached_input_fen_per_million = u64::MAX;
    inputs.rate_card.cached_input_fen_per_million = u64::MAX;
    inputs.rate_card.cache_write_input_fen_per_million = u64::MAX;
    inputs.rate_card.output_fen_per_million = u64::MAX;

    let exact_max = Usage {
        total_tokens: 1_000_000,
        input_tokens: 1_000_000,
        ..Usage::default()
    };
    assert_eq!(
        calculate_cost(&calculation_input(&inputs, &exact_max))
            .unwrap()
            .estimated_fen,
        u64::MAX
    );

    let above_max = Usage {
        total_tokens: 1_000_001,
        input_tokens: 1_000_001,
        ..Usage::default()
    };
    let error = calculate_cost(&calculation_input(&inputs, &above_max)).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("rounded estimated cost does not fit u64"),
        "{error:#}"
    );

    let maximal_intermediate = Usage {
        total_tokens: i64::MAX,
        input_tokens: i64::MAX / 2 + 1,
        cached_input_tokens: i64::MAX / 4,
        cache_write_input_tokens: i64::MAX / 4,
        output_tokens: i64::MAX / 2,
        reasoning_output_tokens: i64::MAX / 2,
    };
    let error = calculate_cost(&calculation_input(&inputs, &maximal_intermediate)).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("rounded estimated cost does not fit u64"),
        "{error:#}"
    );
}

#[test]
fn cost_arithmetic_preserves_supplier_actual_and_charges_the_greater_amount() {
    let inputs = verified_inputs();
    let usage = Usage {
        total_tokens: 1_000_000,
        input_tokens: 1_000_000,
        ..Usage::default()
    };
    let cases = [
        ("absent", None, 12),
        ("lower", Some(11), 12),
        ("equal", Some(12), 12),
        ("higher", Some(13), 13),
    ];

    for (name, supplier_actual_fen, charged_fen) in cases {
        let mut input = calculation_input(&inputs, &usage);
        input.supplier_actual_fen = supplier_actual_fen;
        let actual = calculate_cost(&input).unwrap_or_else(|error| panic!("{name}: {error:#}"));
        assert_eq!(
            actual,
            CalculatedCost {
                estimated_fen: 12,
                supplier_actual_fen,
                charged_fen,
                within_ceilings: true,
            },
            "{name}"
        );
    }
}

#[test]
fn cost_arithmetic_marks_exact_and_exceeded_post_run_ceilings_without_refusal() {
    let inputs = verified_inputs();
    let usage = Usage {
        total_tokens: 1_000_000,
        input_tokens: 1_000_000,
        ..Usage::default()
    };
    let cases = [
        ("exactly at both", 1_000_000, 13, true),
        ("above token ceiling", 999_999, 13, false),
        ("above charged ceiling", 1_000_000, 12, false),
        ("above both", 999_999, 12, false),
    ];

    for (name, max_total_tokens, approved_per_run_fen, within_ceilings) in cases {
        let mut input = calculation_input(&inputs, &usage);
        input.supplier_actual_fen = Some(13);
        input.max_total_tokens = max_total_tokens;
        input.approved_per_run_fen = approved_per_run_fen;
        let actual = calculate_cost(&input).unwrap_or_else(|error| panic!("{name}: {error:#}"));
        assert_eq!(actual.within_ceilings, within_ceilings, "{name}");
    }
}
