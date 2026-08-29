use crate::cost_contracts::VerifiedCostInputs;
use crate::model::Usage;

pub(crate) struct CostCalculationInput<'a> {
    pub(crate) inputs: &'a VerifiedCostInputs,
    pub(crate) usage: &'a Usage,
    pub(crate) supplier_actual_fen: Option<u64>,
    pub(crate) approved_per_run_fen: u64,
    pub(crate) max_total_tokens: u64,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct CalculatedCost {
    pub(crate) estimated_fen: u64,
    pub(crate) supplier_actual_fen: Option<u64>,
    pub(crate) charged_fen: u64,
    pub(crate) within_ceilings: bool,
}

pub(crate) fn calculate_cost(input: &CostCalculationInput<'_>) -> anyhow::Result<CalculatedCost> {
    let usage = input.usage;
    for (field, value) in [
        ("total_tokens", usage.total_tokens),
        ("input_tokens", usage.input_tokens),
        ("cached_input_tokens", usage.cached_input_tokens),
        (
            "cache_write_input_tokens",
            usage.cache_write_input_tokens,
        ),
        ("output_tokens", usage.output_tokens),
        ("reasoning_output_tokens", usage.reasoning_output_tokens),
    ] {
        if value < 0 {
            anyhow::bail!("{field} cannot be negative")
        }
    }

    let expected_total = usage
        .input_tokens
        .checked_add(usage.output_tokens)
        .ok_or_else(|| anyhow::anyhow!("input and output token addition overflow"))?;
    if usage.total_tokens != expected_total {
        anyhow::bail!("total_tokens must equal input_tokens plus output_tokens")
    }
    if usage.reasoning_output_tokens > usage.output_tokens {
        anyhow::bail!("reasoning_output_tokens cannot exceed output_tokens")
    }

    let uncached_input_tokens = usage
        .input_tokens
        .checked_sub(usage.cached_input_tokens)
        .and_then(|value| value.checked_sub(usage.cache_write_input_tokens))
        .ok_or_else(|| anyhow::anyhow!("input token subtraction overflow"))?;
    if uncached_input_tokens < 0 {
        anyhow::bail!("cached and cache-write tokens cannot exceed input_tokens")
    }

    let uncached = u128::try_from(uncached_input_tokens)
        .map_err(|_| anyhow::anyhow!("uncached input token conversion failed"))?;
    let cached = u128::try_from(usage.cached_input_tokens)
        .map_err(|_| anyhow::anyhow!("cached input token conversion failed"))?;
    let cache_write = u128::try_from(usage.cache_write_input_tokens)
        .map_err(|_| anyhow::anyhow!("cache-write input token conversion failed"))?;
    let output = u128::try_from(usage.output_tokens)
        .map_err(|_| anyhow::anyhow!("output token conversion failed"))?;
    let rate = &input.inputs.rate_card;
    let uncached_cost = uncached
        .checked_mul(u128::from(rate.uncached_input_fen_per_million))
        .ok_or_else(|| anyhow::anyhow!("uncached cost overflow"))?;
    let cached_cost = cached
        .checked_mul(u128::from(rate.cached_input_fen_per_million))
        .ok_or_else(|| anyhow::anyhow!("cached cost overflow"))?;
    let cache_write_cost = cache_write
        .checked_mul(u128::from(rate.cache_write_input_fen_per_million))
        .ok_or_else(|| anyhow::anyhow!("cache-write cost overflow"))?;
    let output_cost = output
        .checked_mul(u128::from(rate.output_fen_per_million))
        .ok_or_else(|| anyhow::anyhow!("output cost overflow"))?;
    let numerator = uncached_cost
        .checked_add(cached_cost)
        .and_then(|value| value.checked_add(cache_write_cost))
        .and_then(|value| value.checked_add(output_cost))
        .ok_or_else(|| anyhow::anyhow!("total cost overflow"))?;
    let rounded = numerator
        .checked_add(999_999)
        .ok_or_else(|| anyhow::anyhow!("cost rounding overflow"))?;
    let estimated_fen = u64::try_from(rounded / 1_000_000)
        .map_err(|_| anyhow::anyhow!("rounded estimated cost does not fit u64"))?;
    let charged_fen = estimated_fen.max(input.supplier_actual_fen.unwrap_or(0));
    let total_tokens = u64::try_from(usage.total_tokens)
        .map_err(|_| anyhow::anyhow!("total token conversion failed"))?;

    Ok(CalculatedCost {
        estimated_fen,
        supplier_actual_fen: input.supplier_actual_fen,
        charged_fen,
        within_ceilings: total_tokens <= input.max_total_tokens
            && charged_fen <= input.approved_per_run_fen,
    })
}
