# sled

File-based AI dialog runner.

A dialog is a directory. Every message is a file.

Built for direct, hands-on work with models: a quiet workbench for unhurried research dialog where simplicity and observability matter. sled deliberately trades scale for legibility: no concurrent users, no parallel runs, no server. Keep the practical limits of that architecture in mind.

The filenames show whose turn it is and what is in flight. `ls` shows you the whole run, and a text editor lets you inspect, repair, or replay any step. There is nothing else: no database, no separate state file, no in-memory state that survives the process.

Each filled message is a JSON5 file named by slot, role, and status:

```text
0001.user.done.json5
0002.running.json5
0003.tool.pending.json5
0004.tool.needs-input.json5
```

A `running` slot has no role yet because the model may write either an assistant message or a tool call. A `needs-input` slot has a role because the target is already known: `0001.user.needs-input.json5` waits for a user message, and `0004.tool.needs-input.json5` waits for an answer that completes the same tool file. After content is written, the role never changes.

Only one non-terminal file may exist at a time: `running`, `pending`, or `needs-input`. The status names who must act:

- `running` — the model is taking its turn
- `pending` — the runner must finish a tool call
- `needs-input` — you must reply, either to the dialog or to a suspended tool
- `done` — closed

### Guarantees

At most one non-terminal file may exist in a dialog. If the runner sees more than one, it exits with an error and touches nothing. A message body or tool result is fully written before the file moves to its next status; a status change is a single atomic `rename`.

Restart `run` after a crash and the runner continues from the one non-terminal file. A filled `running` file is closed. A `pending` tool file with a result is closed. A `pending` tool file with a suspension request becomes `tool.needs-input`. A `pending` tool file without either is executed, so tools with side effects must be idempotent across crash recovery.

An empty `running` slot has only a number and status. A `needs-input` slot has a role because it names who needs the human input. Once content is written, number and role do not change; only status changes.

## Contents

- [Quick Start](#quick-start)
- [Commands](#commands)
- [Config](#config)
- [Dialog Config](#dialog-config)
- [System Prompt](#system-prompt)
- [Context](#context)
- [Tools](#tools)
- [Workspace](#workspace)
- [Logging](#logging)
- [Customization](#customization)

## Quick Start

If `cargo` is not installed yet, install the Rust toolchain from the official [Rust install page](https://www.rust-lang.org/tools/install). `cargo` is installed with Rust.

You usually work in a `say` / `run` loop: `say` writes what you say to whoever is waiting, and `run` lets the model react until it finishes, asks for input, or needs a tool result.

Create a dialog and add a user message:

```bash
cargo run -p sled-cli -- say ./dialog "Summarize https://example.com"
```

Run the assistant locally with `operator` first. It needs no API key and lets you try the file protocol directly:

```bash
cargo run -p sled-cli -- run ./dialog --provider operator
```

Or run with the default OpenAI provider:

```bash
export OPENAI_API_KEY=...
cargo run -p sled-cli -- run ./dialog
```

Look at the whole run:

```bash
ls -1 ./dialog
cargo run -p sled-cli -- status ./dialog
```

Inspect the exact context sent to the model:

```bash
cargo run -p sled-cli -- context ./dialog
```

## Commands

Use `cargo run -p sled-cli -- <command>` during development.

- `init <dir>` — create the dialog directory, `_system.json5`, and `_config.json5`. Optional.
  - `--system <text>` to set custom system instructions.
  - `--system-file <path>` to read custom system instructions from a file.
- `say <dir> <text>` — send text to whoever is waiting. With no open file, it creates a user message. With `user.needs-input`, it fills a user reply. With `tool.needs-input`, it fills the suspended tool answer.
  - `--run` to start the runner immediately after writing the message, using the same config/defaults as `run`.
  - `--body-mirror` to save markdown body mirrors as enabled. Default: off.
- `run <dir>` — continue execution until done, needs-input, or error.
  - `--provider <operator|openai|openai-compatible|anthropic>` to set the provider. Default: `openai`.
  - `--model <model>` to set the provider model. Defaults: `openai=gpt-5.5`, `anthropic=claude-sonnet-4-6`; `openai-compatible` requires one.
  - `--openai-compatible-base-url <url>` for `openai-compatible`.
  - `--all` to use the full message context. Default.
  - `--recent-messages <n>` to use `recent-messages`.
  - `--recent-bytes <bytes>` to use `recent-bytes`.
  - `--body-mirror` to save markdown body mirrors as enabled. Default: off.
- `context <dir>` — show the exact prompt, index, and bodies sent to the model.
- `status <dir>` — print the current non-terminal file if one exists, plus the latest message.
- `config <dir>` — create or update `_config.json5`.
  - `--provider <operator|openai|openai-compatible|anthropic>` to save a provider override. Default: `openai`.
  - `--model <model>` to save a model override. Defaults: `openai=gpt-5.5`, `anthropic=claude-sonnet-4-6`; `openai-compatible` requires one.
  - `--openai-compatible-base-url <url>` for `openai-compatible`.
  - `--all` to save full message context by clearing context limits.
  - `--recent-messages <n>` to select `recent-messages` and set its limit.
  - `--recent-bytes <bytes>` to select `recent-bytes` and set its limit.
  - `--body-mirror` to save markdown body mirrors as enabled. Default: off.

Every command has help:

```bash
cargo run -p sled-cli -- --help
cargo run -p sled-cli -- run --help
```

## Config

Copy `.env.example` to `.env` and fill the values you need:

```bash
cp .env.example .env
```

API keys are read from `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, or `SLED_OPENAI_COMPAT_API_KEY` depending on the provider. Secrets stay in env; do not put API keys in dialog files.

Runtime options are resolved in this order:

1. Command-line arguments.
2. Dialog-local `_config.json5`.
3. Built-in defaults.

CLI arguments override `_config.json5` for the current command. Missing config keys fall back to built-in defaults. `init`, `say`, `run`, and `context` create `_config.json5` if it is missing, filled with the current resolved settings. They do not rewrite an existing config.

Use `openai-compatible` for providers that expose the OpenAI Chat Completions shape:

```bash
SLED_OPENAI_COMPAT_API_KEY=...
cargo run -p sled-cli -- config ./dialog \
  --provider openai-compatible \
  --model openai/gpt-4o-mini \
  --openai-compatible-base-url https://openrouter.ai/api/v1
```

Env is only for secrets. Put non-secret runtime settings in `_config.json5` or pass them as command-line arguments.

## Dialog Config

Each dialog may have `_config.json5` for local, non-secret runtime settings. The file may be partial; missing keys use built-in defaults. Command-line arguments override it for the current command.

```json5
{
  provider: "openai-compatible",
  model: "openai/gpt-4o-mini",
  openai_compatible_base_url: "https://openrouter.ai/api/v1",
  recent_messages: 8,
  body_mirror: true,
}
```

Supported keys:

- `provider`: `operator`, `openai`, `openai-compatible`, or `anthropic`.
- `model`: provider model name.
- `openai_compatible_base_url`: base URL for `openai-compatible`, such as `https://openrouter.ai/api/v1`.
- `recent_messages`: include only the last `n` message bodies.
- `recent_bytes`: include the newest body sections that fit in this byte budget.
- `body_mirror`: write readable `.done.md` mirrors beside JSON5 files.

If neither `recent_messages` nor `recent_bytes` is set, sled uses the full message context.

`_config.json5` is the per-dialog runtime config. Use `config` to persist settings, command-line arguments for command-local overrides, and env only for secrets.

You can write it from the CLI:

```bash
cargo run -p sled-cli -- config ./dialog \
  --provider openai-compatible \
  --model openai/gpt-4o-mini \
  --openai-compatible-base-url https://openrouter.ai/api/v1 \
  --recent-messages 8 \
  --body-mirror
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
cargo run -p sled-cli -- init ./dialog --system "Be concise."
cargo run -p sled-cli -- init ./dialog --system-file ./system.md
```

Built-in sled protocol prompts are always included. `_system.json5` only appends dialog-specific instructions.

## Context

By default, the `all` fold sends every message body in the model context:

```bash
cargo run -p sled-cli -- context ./dialog
```

Use the `recent-messages` fold to include only the last `n` messages in the index and bodies:

```bash
cargo run -p sled-cli -- config ./dialog --recent-messages 4
cargo run -p sled-cli -- context ./dialog
cargo run -p sled-cli -- run ./dialog
```

`RecentMessagesFold` limits both the file index and message bodies. The model can still use the `open` tool for slot numbers it already knows, but hidden slots are not advertised in the current context.

Use `recent-bytes` when the limit should be approximate size rather than message count:

```bash
cargo run -p sled-cli -- config ./dialog --recent-bytes 120000
cargo run -p sled-cli -- context ./dialog
```

`RecentBytesFold` adds body sections from newest to oldest until the next section would exceed the byte budget, and includes index rows only for those selected sections. The budget applies only to `bodies`; the system prompt and selected index rows are outside that limit.

## Tools

Tool files are executed sequentially by the runner: one `tool.pending` file at a time, in slot order. A single tool may still batch work internally — the protocol prompt instructs the model to put one batched request (several paths, several URLs) into one tool call whenever the next step does not depend on each intermediate result, so a sequential protocol does not mean one file per item.

Each tool request and its result live in the same file. A completed tool is renamed from `tool.pending` to `tool.done`. A suspending tool writes a request for human input and becomes `tool.needs-input`; `say` or a manual edit fills the answer, then the same file becomes `tool.done`.

Built-in tools:

- `open`: open older message bodies by slot number.
- `read`: read local filesystem files.
- `http_get`: fetch HTTP/HTTPS URLs with timeout and response-size limits. Redirects are not followed, and local/private IP targets are rejected.

`read` intentionally has no path sandbox. sled is built for a trusted, single-user local workspace where the person running the tool controls the files it can inspect.

## Workspace

- `sled-core`: file protocol, status transitions, fold trait, runner.
- `sled-fold`: context fold implementations: `all`, `recent-messages`, and `recent-bytes`.
- `sled-ai`: assistant providers.
- `sled-tools`: sequential tool registry and built-in tools, one tool per source file.
- `sled-cli`: command-line interface.

## Logging

Logging uses `tracing` and is controlled by `RUST_LOG`:

```bash
RUST_LOG=info cargo run -p sled-cli -- run ./dialog
RUST_LOG=sled_core=debug,sled_ai=debug cargo run -p sled-cli -- run ./dialog
```

The default level is `warn`. API keys and full model context are not logged by default.

## Customization

The two main control points are tools, which let the model act, and folds, which decide what the model can see.

### Adding a Tool

Add a tool when the model needs a new action. Each built-in tool in `sled-tools` has its own source file. Implement the `Tool` trait and return `ToolResult::Completed(value)` for a normal result or `ToolResult::Suspended(request)` when a human must answer before the tool call can finish. Register the tool in a `ToolRegistry`, pass it through a `Profile`, and add a protocol prompt fragment so the model knows how to call it.

### Adding a Fold

Add a fold when the model should see a different representation of the dialog. Folds implement `sled_core::Fold` and live in `sled-fold`. A fold receives the scanned slots and returns `Context { index, bodies }`; this is the only place that decides how the directory becomes model context. Existing examples are `AllFold`, `RecentMessagesFold`, and `RecentBytesFold`.
