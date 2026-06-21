use anyhow::{Result, bail};
use sled_core::Fold;
use sled_fold::{AllFold, FoldPipeline, RecentBytesFold, RecentMessagesFold, RecentTokensFold};

pub(crate) fn build_fold_pipeline(spec: &str) -> Result<Box<dyn Fold>> {
    let stages = parse_stages(spec)?;
    let source = build_source(stages[0])?;

    for stage in stages.iter().skip(1) {
        if is_source_stage(stage) {
            bail!("fold stage `{stage}` can only appear first");
        }
        bail!("unknown fold stage `{stage}`");
    }

    Ok(Box::new(FoldPipeline::new(source)))
}

fn parse_stages(spec: &str) -> Result<Vec<&str>> {
    let spec = spec.trim();
    if spec.is_empty() {
        bail!("fold pipeline cannot be empty");
    }
    let stages = spec
        .split(',')
        .map(str::trim)
        .map(|stage| {
            if stage.is_empty() {
                bail!("fold pipeline contains an empty stage");
            }
            Ok(stage)
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(stages)
}

fn build_source(stage: &str) -> Result<Box<dyn Fold>> {
    if stage == "all" {
        return Ok(Box::new(AllFold));
    }
    if let Some(value) = stage.strip_prefix("recent-messages:") {
        return Ok(Box::new(RecentMessagesFold::new(parse_positive_usize(
            "recent-messages",
            value,
        )?)));
    }
    if let Some(value) = stage.strip_prefix("recent-bytes:") {
        return Ok(Box::new(RecentBytesFold::new(parse_positive_usize(
            "recent-bytes",
            value,
        )?)));
    }
    if let Some(value) = stage.strip_prefix("recent-tokens:") {
        return Ok(Box::new(RecentTokensFold::new(parse_positive_usize(
            "recent-tokens",
            value,
        )?)));
    }
    if stage.starts_with("recent-messages")
        || stage.starts_with("recent-bytes")
        || stage.starts_with("recent-tokens")
    {
        bail!("fold stage `{stage}` requires a positive numeric limit after `:`");
    }
    bail!(
        "first fold stage must be one of: all, recent-messages:N, recent-bytes:N, recent-tokens:N"
    )
}

fn parse_positive_usize(name: &str, value: &str) -> Result<usize> {
    let parsed = value
        .parse::<usize>()
        .map_err(|_| anyhow::anyhow!("fold stage `{name}` requires a positive numeric limit"))?;
    if parsed == 0 {
        bail!("fold stage `{name}` requires a positive numeric limit");
    }
    Ok(parsed)
}

fn is_source_stage(stage: &str) -> bool {
    stage == "all"
        || stage.starts_with("recent-messages")
        || stage.starts_with("recent-bytes")
        || stage.starts_with("recent-tokens")
}
