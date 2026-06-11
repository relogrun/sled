# sled - file-based AI dialog runner

A dialog is a directory. Every message is a file. Status changes are atomic.

It is built for direct, hands-on work with models when you want to inspect, edit, or replay a research dialog, run model work from scripts or CI, or see the exact context sent to the model.

`sled` is intentionally simple: one user, no parallel runs, no server. The filenames show whose turn it is and what is in flight.

`ls` shows the whole run, and a text editor lets you inspect, repair, or replay any step. There is nothing else: no database, no separate state file, no in-memory state that survives the process.

Each filled message is a JSON5 file named by slot, role, and status:

```text
0001.user.done.json5
0002.assistant.done.json5
0003.tool.pending.json5
0004.tool.needs-input.json5
```

### Guarantees

- At most one non-terminal file may exist: `running`, `pending`, or `needs-input`. If the runner sees more than one, it exits with an error and touches nothing.
- Content is durably written before status changes. A status change is a single atomic `rename`.
- A crash between write and rename is recoverable. The next `run` completes the visible old-status file.
- A pending tool with no result or suspension may be executed again after a crash. Side-effectful tools must be idempotent.
- Once content is written, slot number and role do not change. Only status changes.

## Contents

- [Quick Start](#quick-start)
- [File Roles and Statuses](#file-roles-and-statuses)
- [Commands](#commands)
- [Config](#config)
- [Dialog Config](#dialog-config)
- [System Prompt](#system-prompt)
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

When a run stops at `needs-input`, answer with `say` and continue with `run`:

```bash
cargo run -p sled-cli -- say ./dialog "Use option A."
cargo run -p sled-cli -- run ./dialog
```

Or do both in one command:

```bash
cargo run -p sled-cli -- say ./dialog "Use option A." --run
```

## File Roles and Statuses

Open slots and filled messages use these filename shapes:

```text
0002.running.json5              # model turn, role not known yet
0001.user.done.json5            # user message
0002.assistant.done.json5       # assistant message
0003.tool.pending.json5         # tool call waiting for the runner
0003.tool.done.json5            # completed tool call
0004.user.needs-input.json5     # dialog waits for the next user message
0004.tool.needs-input.json5     # suspended tool waits for a human answer
```

An open model turn is roleless because the model may write either an assistant message or a tool call. Once content is written, the role never changes.

The status names who must act:

- `running` — the model is taking its turn
- `pending` — the runner must finish a tool call
- `needs-input` — you must reply, either to the dialog or to a suspended tool
- `done` — closed

## Commands

Use `cargo run -p sled-cli -- <command>` during development.

- `init <dir>` — create the dialog directory and `_system.json5`. Optional.
  - `--system <text>` to set custom system instructions.
  - `--system-file <path>` to read custom system instructions from a file.
- `say <dir> <text>` — send text to whoever is waiting. With no open file, it creates a user message. With `user.needs-input`, it fills a user reply. With `tool.needs-input`, it writes the suspended tool result.
  - `--run` to start the runner immediately after writing the message, using the same config/defaults as `run`.
  - `--body-mirror` to save markdown body mirrors as enabled. Default: off.
- `run <dir>` — continue execution until done, needs-input, or error.
  - `--provider <operator|openai|openai-compatible|anthropic>` to set the provider. Default: `openai`.
  - `--model <model>` to set the selected provider's model. Defaults: `openai=gpt-5.5` and `anthropic=claude-sonnet-4-6`. `openai-compatible` requires one.
  - `--openai-compatible-base-url <url>` for `openai-compatible`.
  - `--all` to use the full message context. Default.
  - `--recent-messages <n>` to use `recent-messages`.
  - `--recent-bytes <bytes>` to use `recent-bytes`.
  - `--body-mirror` to save markdown body mirrors as enabled. Default: off.
- `context <dir>` — show the exact prompt, index, and bodies sent to the model.
- `status <dir>` — print the current non-terminal file if one exists, plus the latest message.
- `config <dir>` — create or update `_config.json5`.
  - `--provider <operator|openai|openai-compatible|anthropic>` to save a provider override. If absent, the runtime default is `openai`.
  - `--model <model>` to save a model override for the selected provider.
  - `--openai-compatible-base-url <url>` for `openai-compatible`.
  - `--all` to save full message context by clearing context limits.
  - `--recent-messages <n>` to select `recent-messages` and set its limit.
  - `--recent-bytes <bytes>` to select `recent-bytes` and set its limit.
  - `--body-mirror` to save markdown body mirrors as enabled.

Every command has help:

```bash
cargo run -p sled-cli -- --help
cargo run -p sled-cli -- run --help
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

Each dialog may have `_config.json5` for local, non-secret runtime overrides. The file may be partial, and missing keys use built-in defaults. Command-line arguments override it for the current command and are not written back. `config <dir>` creates or updates the file explicitly. If the file is absent, `say`, `run`, and `context` use defaults without creating it.

```json5
{
  provider: "openai-compatible",
  openai: {
    model: "gpt-5.5",
  },
  anthropic: {
    model: "claude-sonnet-4-6",
  },
  openai_compatible: {
    model: "openai/gpt-4o-mini",
    base_url: "https://openrouter.ai/api/v1",
  },
  recent_messages: 8,
  body_mirror: true,
}
```

Supported keys:

- `provider`: `operator`, `openai`, `openai-compatible`, or `anthropic`.
- `openai.model`: OpenAI model name.
- `anthropic.model`: Anthropic model name.
- `openai_compatible.model`: model name for an OpenAI-compatible provider.
- `openai_compatible.base_url`: base URL for `openai-compatible`, such as `https://openrouter.ai/api/v1`.
- `recent_messages`: include only the last `n` message bodies.
- `recent_bytes`: include the newest body sections that fit in this byte budget.
- `body_mirror`: write readable `.done.md` mirrors beside JSON5 files.

Runtime defaults: provider is `openai`, OpenAI model is `gpt-5.5`, Anthropic model is `claude-sonnet-4-6`, body mirrors are off, and `openai-compatible` requires both `openai_compatible.model` and `openai_compatible.base_url`.

If neither `recent_messages` nor `recent_bytes` is set, sled uses the full message context.

Write the file from the CLI when that is easier:

```bash
cargo run -p sled-cli -- config ./dialog --recent-messages 8 --body-mirror
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

## Tools

Tool files are executed sequentially by the runner: one `tool.pending` file at a time, in slot order. A model turn can request one tool call. A single tool call may still batch work internally — the protocol prompt instructs the model to put one batched request (several paths, several URLs) into one tool call whenever the next step does not depend on each intermediate result, so a sequential protocol does not mean one file per item.

Each tool request and its result live in the same file. A completed tool is renamed from `tool.pending` to `tool.done`. A suspending tool writes a request for human input and becomes `tool.needs-input`. Then `say` or a manual edit writes the result, and the same file becomes `tool.done`.

Built-in tools:

- `open`: open older message bodies by slot number.
- `read`: read local filesystem files.
- `http_get`: fetch HTTP/HTTPS URLs with timeout and response-size limits. Redirects are not followed, and local/private IP targets are rejected.
- `escalate`: ask the human for input when the model cannot continue without a decision or answer. This suspends the tool call as `tool.needs-input`.

`user.needs-input` and `tool.needs-input` are different handoffs. A `user.needs-input` file asks for the next user message in the dialog. A `tool.needs-input` file belongs to an already-started tool call. When you answer it with `say`, sled writes a tool `result`, closes the same file as `tool.done`, and the model continues from that result.

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

Add a tool when the model needs a new action. Each built-in tool in `sled-tools` has its own source file. Implement the `Tool` trait and return `ToolResult::Completed(value)` for a normal result or `ToolResult::Suspended(request)` when a human must answer before the tool call can finish. Register the tool in a `ToolRegistry` and pass it through a `Profile`. Put the tool instructions in `_system.json5` with `init --system`, `init --system-file`, or a manual edit so the model knows how to call it. Start from `ToolRegistry::with_defaults()` to include built-ins, or `ToolRegistry::new()` to exclude them. See `crates/sled-cli/examples/custom_profile.rs` for a minimal custom binary.

### Adding a Fold

Add a fold when the model should see a different representation of the dialog. Folds implement `sled_core::Fold` and live in `sled-fold`. A fold receives the scanned slots and returns `Context { index, bodies }`. This is the only place that decides how the directory becomes model context. Existing examples are `AllFold`, `RecentMessagesFold`, and `RecentBytesFold`.
