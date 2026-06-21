# sled - file-based AI dialog runner

A dialog is a directory. Every message is a file. Status changes are atomic.

It is built for direct, hands-on work with models when you want to inspect, edit, or replay a research dialog, run model work from scripts or CI, or inspect the assembled model input.

`sled` is intentionally simple: one user, no parallel runs, no server. The filenames show whose turn it is and what is in flight.

`ls` shows the whole run, and a text editor lets you inspect, repair, or replay any step. There is nothing else: no database, no separate state file, no in-memory state that survives the process.

Each filled message is a JSON5 file named by slot, role, and status:

```text
0001.user.done.json5       # user message, closed
0002.assistant.done.json5  # assistant message, closed
0003.tool.pending.json5    # tool call waiting for the runner
0004.tool.awaiting.json5   # suspended tool awaiting input from you
```

### Guarantees

- At most one non-terminal file may exist: `running`, `pending`, or `awaiting`. If the runner sees more than one, it exits with an error and touches nothing.
- Content is durably written before status changes. A status change is a single atomic `rename`.
- A crash between write and rename is recoverable. The next `run` completes the visible old-status file.
- A pending tool with no result or suspension may be executed again after a crash. Side-effectful tools must be idempotent.
- Once content is written, slot number and role do not change. Only status changes.

## Contents

- [Quick Start](#quick-start)
- [Core Ideas](#core-ideas)
- [File Roles and Statuses](#file-roles-and-statuses)
- [Commands](#commands)
- [Config](#config)
- [Dialog Config](#dialog-config)
- [System Prompt](#system-prompt)
- [Compaction](#compaction)
- [Tools](#tools)
- [Workspace](#workspace)
- [Logging](#logging)
- [Customization](#customization)

## Quick Start

If `cargo` is not installed yet, install the Rust toolchain from the official [Rust install page](https://www.rust-lang.org/tools/install). `cargo` is installed with Rust.

Examples below assume a `sled` binary is available on `PATH`. During local development, replace `sled <command>` with `cargo run -p sled-cli -- <command>`.

You usually work in a `say` / `run` loop: `say` writes what you say to whoever is waiting, and `run` lets the model react until it finishes, asks for input, or needs a tool result.

Create a dialog and add a user message:

```bash
sled say ./runs/example "Summarize https://example.com"
```

Run the assistant locally with `operator` first. It needs no API key and lets you try the file protocol directly:

```bash
sled run ./runs/example --provider operator
```

Or run with the default OpenAI provider:

```bash
export OPENAI_API_KEY=...
sled run ./runs/example
```

Look at the whole run:

```bash
ls -1 ./runs/example
sled status ./runs/example
```

Inspect the assembled system prompt, index, and bodies for the current dialog files:

```bash
sled context ./runs/example
```

When a run stops at `awaiting`, answer with `say` and continue with `run`:

```bash
sled say ./runs/example "Use option A."
sled run ./runs/example
```

Or do both in one command:

```bash
sled say ./runs/example "Use option A." --run
```

## Core Ideas

The normal workflow is a `say` / `run` loop. `say` writes the next user or human answer into the dialog directory. `run` advances the dialog until the assistant finishes, asks for user input, requests a tool, or hits an error.

The system has four main moving parts:

- Files are the source of truth. A dialog directory contains messages, open work, config, system instructions, and optional archive data.
- Tools let the model act. A model writes one `tool.pending` request; sled executes it, records the result in the same file, and continues.
- Folds decide what the model can see. A fold turns the current dialog files into `index` and `bodies`, then the common context budget keeps the newest whole body sections that fit.
- Compact rewrites old dialog files into a summarized `compact` message and archives the originals. It is a storage operation that makes later folds smaller.

You can inspect every layer directly: list files, open JSON5 messages, run `context` to see the exact model input, and use `archive` tooling to recover compacted details.

## File Roles and Statuses

Open slots and filled messages use these filename shapes:

```text
0002.running.json5          # model turn, role not known yet
0001.user.done.json5        # user message, closed
0002.assistant.done.json5   # assistant message, closed
0001.compact.done.json5     # compacted summary replacing archived old messages
0003.tool.pending.json5     # tool call awaiting the runner
0003.tool.done.json5        # tool call completed by the runner
0004.tool.awaiting.json5    # suspended tool awaiting input from you
0005.user.awaiting.json5    # dialog awaiting your next message
```

An open model turn is roleless because the model may write either an assistant message or a tool call. Once content is written, the role never changes.

## Commands

Use `sled <command>` after installing or building the binary.

- `init <dir>` — create the dialog directory and `_system.json5`. Optional.
  - `--system <text>` to set custom system instructions.
  - `--system-file <path>` to read custom system instructions from a file.
- `say <dir> <text>` — send text to whoever is waiting. With no open file, it creates a user message. With `user.awaiting`, it fills a user reply. With `tool.awaiting`, it writes the suspended tool result.
  - `--run` to start the runner immediately after writing the message, using the same config/defaults as `run`.
  - `--body-mirror` to save markdown body mirrors as enabled. Default: off.
- `run <dir>` — continue execution until done, awaiting, or error.
  - `--provider <operator|openai|openai-compatible|anthropic>` to set the provider. Default: `openai`.
  - `--model <model>` to set the selected provider's model. Defaults: `openai=gpt-5.4-mini` and `anthropic=claude-sonnet-4-6`. `openai-compatible` requires one.
  - `--openai-reasoning <minimal|low|medium|high>` to set OpenAI reasoning effort for this run.
  - `--anthropic-effort <low|medium|high|xhigh|max>` to set Anthropic effort for this run.
  - `--anthropic-thinking <off|adaptive>` to set Anthropic thinking mode for this run.
  - `--openai-compatible-base-url <url>` for `openai-compatible`.
  - `--fold <pipeline>` to choose the fold pipeline. Default: `all`. Examples: `all`, `recent-messages:8`, `recent-tokens:50000`.
  - `--context-window-tokens <n>` to override the model context window used for the safety budget.
  - `--context-ratio <ratio>` to override the max ratio of the model context window used by input. Default: `0.8`.
  - `--body-mirror` to save markdown body mirrors as enabled. Default: off.
- `compact <dir>` — summarize selected active `done` slots through a model, archive their original files, and replace them with one `compact` message.
  - `--from-slot <n>` to start compaction at a specific slot. Default: first active `done` slot.
  - Use exactly one range end:
    - `--to-slot <n>` to compact through a slot number.
    - `--keep-recent <n>` to leave the last `n` active `done` slots raw.
    - `--keep-recent-tokens <n>` to leave the newest active `done` body sections fitting this estimated token budget raw.
  - `--summary-tokens <n>` to set the target summary size. Must be greater than zero. Default: `2000`.
  - Provider/model/context options are the same as `run`, except `compact` does not take `--fold`.
- `context <dir>` — show the assembled system prompt, index, and bodies for the current dialog files.
  - `--fold <pipeline>` to override the fold pipeline used for the displayed context.
  - `--context-window-tokens <n>` to override the model context window used for the displayed safety budget.
  - `--context-ratio <ratio>` to override the max ratio used for the displayed input.
- `status <dir>` — print the current non-terminal file if one exists, plus the latest message.
- `config <dir>` — create or update `_config.json5`.
  - `--provider <operator|openai|openai-compatible|anthropic>` to save a provider override. If absent, the runtime default is `openai`.
  - `--model <model>` to save a model override for the selected provider.
  - `--openai-reasoning <minimal|low|medium|high>` to save OpenAI reasoning effort.
  - `--anthropic-effort <low|medium|high|xhigh|max>` to save Anthropic effort.
  - `--anthropic-thinking <off|adaptive>` to save Anthropic thinking mode.
  - `--openai-compatible-base-url <url>` for `openai-compatible`.
  - `--fold <pipeline>` to save the fold pipeline.
  - `--context-window-tokens <n>` to save the model context window used for the safety budget.
  - `--context-ratio <ratio>` to save the max ratio of the model context window used by input.
  - `--body-mirror` to save markdown body mirrors as enabled.

Provider-specific flags are validated against the active provider after applying any `--provider` override. For example, `--openai-reasoning` is only valid with `openai`, Anthropic effort/thinking flags are only valid with `anthropic`, and `--model` is not used with `operator`.

Every command has help:

```bash
sled --help
sled run --help
```

## Config

Copy `.env.example` to `.env` when you need API keys:

```bash
cp .env.example .env
```

Secrets stay in env. Runtime options are resolved in this order:

1. Command-line arguments.
2. Dialog-local `_config.json5`.
3. Built-in defaults.

Use env only for secrets: `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, or `SLED_OPENAI_COMPAT_API_KEY`. Put non-secret runtime settings in `_config.json5` or pass them as command-line arguments.

Model HTTP requests are retried conservatively for transient transport failures and statuses such as `429`, `502`, `503`, and `504`. Request-shape errors such as `400`, `413`, or `431` are not retried.

## Dialog Config

Each dialog may have `_config.json5` for local, non-secret runtime overrides. The file may be partial, and missing keys use built-in defaults. Command-line arguments override it for the current command and are not written back. `config <dir>` creates or updates the file explicitly. If the file is absent, `say`, `run`, `compact`, and `context` use defaults without creating it.

```json5
{
  provider: "openai-compatible",
  openai: {
    model: "gpt-5.4-mini",
    reasoning: "low",
  },
  anthropic: {
    model: "claude-sonnet-4-6",
    effort: "medium",
    thinking: "adaptive",
  },
  openai_compatible: {
    model: "openai/gpt-4o-mini",
    base_url: "https://openrouter.ai/api/v1",
  },
  fold: "recent-tokens:20000",
  context_window_tokens: 128000,
  context_ratio: 0.8,
  body_mirror: true,
}
```

Supported keys:

- `provider`: `operator`, `openai`, `openai-compatible`, or `anthropic`.
- `openai.model`: OpenAI model name.
- `openai.reasoning`: OpenAI reasoning effort, one of `minimal`, `low`, `medium`, or `high`.
- `anthropic.model`: Anthropic model name.
- `anthropic.effort`: Anthropic effort, one of `low`, `medium`, `high`, `xhigh`, or `max`.
- `anthropic.thinking`: Anthropic thinking mode, `off` or `adaptive`.
- `openai_compatible.model`: model name for an OpenAI-compatible provider.
- `openai_compatible.base_url`: base URL for `openai-compatible`, such as `https://openrouter.ai/api/v1`.
- `fold`: fold pipeline, one of `all`, `recent-messages:N`, or `recent-tokens:N`.
- `context_window_tokens`: model context window used for the final input safety budget. Must be greater than zero.
- `context_ratio`: max ratio of the model context window used by `system + index + bodies`. Must be greater than zero and less than or equal to one.
- `body_mirror`: write readable `.done.md` mirrors beside JSON5 files.

Runtime defaults:

- `provider`: `openai`
- OpenAI model: `gpt-5.4-mini`
- Anthropic model: `claude-sonnet-4-6`
- `context_window_tokens`: known model context window when available, otherwise `128000`
- `context_ratio`: `0.8`
- `body_mirror`: off
- `openai-compatible` requires both `openai_compatible.model` and `openai_compatible.base_url`

If `fold` is absent, sled uses `all`. Fold pipelines are comma-separated to leave room for later stages; for now the supported forms are single-stage context-producing folds: `all`, `recent-messages:N`, and `recent-tokens:N`.

The final safety budget is common to every fold: sled keeps `system` and `index`, then keeps the newest whole body sections that fit within `context_window_tokens * context_ratio`. If `system + index` alone exceeds the budget, the run fails before sending a model request.

Token budgets are currently estimated, not counted with a model tokenizer. sled approximates tokens from UTF-8 byte length, using roughly four bytes per token. This applies to `recent-tokens:N` and to the final safety budget.

Write the file from the CLI when that is easier:

```bash
sled config ./runs/example --fold recent-messages:8 --body-mirror
```

## System Prompt

Each dialog has `_system.json5`:

```json5
{
  prompt: "Custom system instructions for this dialog."
}
```

You can also set it during init:

```bash
sled init ./runs/example --system "Be concise."
sled init ./runs/example --system-file ./system.md
```

Built-in sled protocol prompts are always included. Tool descriptions from the active `ToolRegistry` are inserted as their own section. `_system.json5` only appends dialog-specific instructions.

## Compaction

`compact` is a storage operation, not a fold. It changes the active dialog so ordinary folds can keep working without special subtraction rules.

For example:

```bash
sled compact ./runs/example --to-slot 42
sled compact ./runs/example --from-slot 10 --to-slot 42
sled compact ./runs/example --keep-recent 8
sled compact ./runs/example --keep-recent-tokens 30000
```

The command sends the selected active `done` slots to the configured model with a compact-specific prompt. It does not send the normal sled protocol prompt or tool descriptions. The compact input is checked against the same context-window settings as `run`; if the selected range does not fit, choose a smaller range.

After a successful compact, originals are moved into the archive and one compact message replaces them in the active dialog:

```text
archive/
  slots/
    0001.user.done.json5
    0002.assistant.done.json5
  compacts/
    0001-0042.json5

0001.compact.done.json5
0043.user.done.json5
```

The active compact message contains the summary shown to future model runs. The manifest in `archive/compacts/` records the compact pass, including the archived slot range, description, provider/model, and a copy of the summary.

## Tools

Tool files are executed sequentially by the runner: one `tool.pending` file at a time, in slot order. A model turn can request one tool call. A single tool call may still batch work internally — the protocol prompt instructs the model to put one batched request (several paths, several URLs) into one tool call whenever the next step does not depend on each intermediate result, so a sequential protocol does not mean one file per item.

Each tool request and its result live in the same file. A completed tool is renamed from `tool.pending` to `tool.done`. A suspending tool writes a request for human input and becomes `tool.awaiting`. Then `say` or a manual edit writes the result, and the same file becomes `tool.done`.

Built-in tools:

- `open`: open older message bodies by slot number.
- `archive`: inspect compacted dialog archive. It can list compact manifests, read one manifest, or read archived slot messages.
- `read`: read local filesystem files.
- `http_get`: fetch HTTP/HTTPS URLs with timeout and response-size limits. Redirects are not followed, and local/private IP targets are rejected.
- `escalate`: ask the human for input when the model cannot continue without a decision or answer. This suspends the tool call as `tool.awaiting`.

`user.awaiting` and `tool.awaiting` are different handoffs. A `user.awaiting` file asks for the next user message in the dialog. A `tool.awaiting` file belongs to an already-started tool call. When you answer it with `say`, sled writes a tool `result`, closes the same file as `tool.done`, and the model continues from that result.

`read` intentionally has no path sandbox. sled is built for a trusted, single-user local workspace where the person running the tool controls the files it can inspect.

## Workspace

- `sled-core`: file protocol, status transitions, fold trait, runner.
- `sled-fold`: context fold implementations: `all`, `recent-messages`, and `recent-tokens`.
- `sled-compact`: LLM compaction and archive layout.
- `sled-ai`: assistant providers.
- `sled-tools`: sequential tool registry and built-in tools, one tool per source file.
- `sled-cli`: command-line interface.

## Logging

Logging uses `tracing` and is controlled by `RUST_LOG`:

```bash
RUST_LOG=info sled run ./runs/example
RUST_LOG=sled_core=debug,sled_ai=debug sled run ./runs/example
```

The default level is `warn`. API keys and full model context are not logged by default.

## Customization

The two main control points are tools, which let the model act, and folds, which decide what the model can see.

### Adding a Tool

Add a tool when the model needs a new action.

- Put the tool in its own source file, like the built-in tools in `sled-tools`.
- Implement the `Tool` trait.
- Return a `description()` string. This is the tool contract shown to the model in the `Available Tools` system prompt section. Include when to use the tool, the expected JSON args, batching rules if any, and what the model should do after the result.
- Return `ToolResult::Completed(value)` for a normal result.
- Return `ToolResult::Suspended(request)` when a human must answer before the tool call can finish.
- Register the tool in a `ToolRegistry` and pass it through a `Profile`.
- Start from `ToolRegistry::with_defaults()` to include built-ins, or `ToolRegistry::new()` to exclude them.
- Put dialog-specific behavior in `_system.json5` with `init --system`, `init --system-file`, or a manual edit. Do not put the basic tool contract there; keep it on the tool's `description()` so every profile that registers the tool exposes the same contract.

See `crates/sled-cli/examples/custom_profile.rs` for a minimal custom binary.

### Adding a Fold

Add a fold when the model should see a different representation of the dialog.

- Implement `sled_core::Fold`.
- Put reusable fold implementations in `sled-fold`.
- Receive the scanned slots and return `Context { index, bodies }`.
- Do context selection or reshaping inside the fold.

Fold output is then passed through the common context safety budget before a model request. Existing examples are `AllFold`, `RecentMessagesFold`, and `RecentTokensFold`.

Use `compact` for storage-level summarization of old dialog files. It rewrites the active dialog before folds read it; it is not a fold implementation.
