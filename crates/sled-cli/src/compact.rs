use crate::args::CompactArgs;
use crate::config::{DialogOptionOverrides, read_resolved_dialog_config};
use anyhow::{Result, bail};
use sled_ai::{ModelOptions, create_model_with_options};
use sled_compact::{
    CompactOptions, CompactRangeEnd, CompactRuntime, archive_slots_dir,
    compact_dialog as compact_dialog_with_model,
};
use std::fs;
use std::path::Path;

pub(crate) async fn compact_dialog(dir: &Path, args: CompactArgs) -> Result<()> {
    fs::create_dir_all(dir)?;
    let config = read_resolved_dialog_config(dir, DialogOptionOverrides::from(&args))?;
    let options = compact_options_from_args(&args)?;
    let model = create_model_with_options(
        config.provider,
        ModelOptions {
            model: config.model.clone(),
            openai_compatible_base_url: config.openai_compatible_base_url.clone(),
            openai_reasoning_effort: config.openai_reasoning_effort,
            anthropic_effort: config.anthropic_effort,
            anthropic_thinking: config.anthropic_thinking,
            temperature: None,
        },
    )?;
    let runtime = CompactRuntime {
        context_limit: config.context_limit,
        provider: config.provider.to_string(),
        model: config.model,
    };
    let manifest = compact_dialog_with_model(dir, model.as_ref(), &runtime, options).await?;
    println!(
        "compacted slots {:04}..{:04} into {:04}.compact.done.json5",
        manifest.from_slot, manifest.to_slot, manifest.compact_slot
    );
    println!(
        "archived {} slots under {}",
        manifest.slots.len(),
        archive_slots_dir(dir).display()
    );
    Ok(())
}

fn compact_options_from_args(args: &CompactArgs) -> Result<CompactOptions> {
    let range_end_count = usize::from(args.to_slot.is_some())
        + usize::from(args.keep_recent.is_some())
        + usize::from(args.keep_recent_tokens.is_some());
    if range_end_count != 1 {
        bail!("use exactly one of --to-slot, --keep-recent, or --keep-recent-tokens");
    }
    let range_end = if let Some(to_slot) = args.to_slot {
        CompactRangeEnd::ToSlot(to_slot)
    } else if let Some(keep_recent) = args.keep_recent {
        CompactRangeEnd::KeepRecent(keep_recent)
    } else {
        CompactRangeEnd::KeepRecentTokens(args.keep_recent_tokens.unwrap())
    };
    let options = CompactOptions {
        from_slot: args.from_slot,
        range_end,
        summary_tokens: args.summary_tokens,
    };
    options.validate()?;
    Ok(options)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_options_require_one_range_end() {
        let args = CompactArgs {
            summary_tokens: sled_compact::DEFAULT_SUMMARY_TOKENS,
            ..CompactArgs::default()
        };
        let err = compact_options_from_args(&args).unwrap_err().to_string();

        assert_eq!(
            err,
            "use exactly one of --to-slot, --keep-recent, or --keep-recent-tokens"
        );
    }
}
