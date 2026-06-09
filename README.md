# sled

File-backed AI dialog runner.

Built for one human and one model: a quiet workbench for unhurried, hands-on research dialog, sled deliberately trades scale for legibility — no concurrent users, no parallel runs, no server.

A dialog is a directory. Every message is a file. The filenames show whose turn it is and what is in flight. `ls` shows you the whole run, and a text editor lets you inspect, repair, or replay any step. There is nothing else: no database, no separate state file, no in-memory state that survives the process. 

Each filled message is a JSON5 file named by slot, role, and status:

```text
0001.user.done.json5
0002.running.json5
0003.tool.pending.json5
```

An open slot has no role yet: `0002.running.json5` or `0004.waiting.json5`. The role is written together with content and never changes after that.

Only one non-terminal file may exist at a time: `running`, `pending`, or `waiting`. The status names who must act:

- `running` — the model is taking its turn
- `pending` — the runner must finish a tool call
- `waiting` — you must reply
- `done` — closed

## Guarantees

- **Content first, then rename.** A message body is always fully written before the file moves to its next status. A status change is a single atomic `rename`.
- **Interrupted runs resume from the cursor.** Kill the process and restart `run`: the runner finds the single non-terminal file and continues from it. A `running` file with content is closed. A `pending` tool file with a result is closed. A `pending` tool file without a result is executed.
- **Tool side effects must be idempotent.** A tool can be re-executed after a crash during execution, before any result was written.
- **Corruption stops, never self-heals.** If the runner ever sees two non-terminal files, it exits with an error and touches nothing. You decide which one is real.
- **Filled message identity is stable.** An empty open slot has only a number and status. Once content is written, the filename gets its role from the message body. After that, number and role do not change; only status changes.

## Quick Start

Create a dialog and add a user message:

```bash
cargo run -p sled-cli -- say ./dialog "Summarize https://example.com"
```

Run the assistant:

```bash
export OPENAI_API_KEY=...
cargo run -p sled-cli -- run ./dialog --provider openai
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

Use `operator` when you want to test the protocol without a network model:

```bash
cargo run -p sled-cli -- run ./dialog --provider operator
```

## Commands

Use `cargo run -p sled-cli -- <command>` during development.

- `init <dir>` — create the dialog directory and `_system.json5` (optional, because `say`/`run` can create it).
- `say <dir> <text>` — add a user message and proceed when the dialog is waiting.
- `run <dir>` — continue execution until done, waiting, or error.
- `context <dir>` — show the exact prompt, index, and bodies sent to the model.
- `status <dir>` — print the current cursor and latest message.

Useful `run` options:

- `--provider <operator|openai|anthropic>`
- `--model <model>`
- `--k <n>` to include only the last `n` message bodies
- `--body-mirror` to write readable `.done.md` mirrors beside JSON5 files

Every command has help:

```bash
cargo run -p sled-cli -- --help
cargo run -p sled-cli -- run --help
```

## Configuration

Copy `.env.example` to `.env` and fill the values you need:

```bash
cp .env.example .env
```

API keys are read from `OPENAI_API_KEY` and `ANTHROPIC_API_KEY`. Provider defaults, model defaults, context limits, and body mirror settings are documented in `.env.example`.

Command-line arguments override `.env` values, for example `--provider`, `--model`, `--k`, and `--body-mirror`.

## System Prompt

Each dialog has `_system.json5`:

```json5
{
  prompt: "Custom system instructions for this dialog."
}
```

Built-in sled protocol prompts are always included. `_system.json5` only appends dialog-specific instructions.

## Context

By default, all message bodies are sent in the model context:

```bash
cargo run -p sled-cli -- context ./dialog
```

Limit context to the last `n` message bodies:

```bash
cargo run -p sled-cli -- context ./dialog --k 4
cargo run -p sled-cli -- run ./dialog --k 4
SLED_RECENT_K=4 cargo run -p sled-cli -- run ./dialog
```

The file index is always included. The `--k` option only limits message bodies. With `--k` set, the model can still reach evicted bodies through the `open` tool, so limiting context loses nothing.

## Tools

Tool files are executed sequentially by the runner: one `tool.pending` file at a time, in slot order. A single tool may still batch work internally — the protocol prompt instructs the model to put one batched request (several paths, several URLs) into one tool call whenever the next step does not depend on each intermediate result, so a sequential protocol does not mean one file per item. Each tool request and its result live in the same `tool.pending` file, which is renamed to `done` after execution.

Built-in tools:

- `open`: open older message bodies by slot number.
- `read`: read local filesystem files.
- `http_get`: fetch HTTP/HTTPS URLs with timeout and response-size limits.

## Body Mirrors

Message bodies always stay inline in JSON5 as the source of truth. Set `SLED_BODY_MIRROR=true` or pass `--body-mirror` to also write readable markdown mirrors:

```bash
cargo run -p sled-cli -- say ./dialog "hello" --body-mirror
```

Mirror names use the final message shape, for example `0002.assistant.done.md`. The runner ignores `.md` mirrors when building context.

Mirrors are write-time projections: if you edit a JSON5 body by hand, its mirror goes stale until the file is written again. Treat mirrors as a view, never as a place to edit.

## Workspace

- `sled-core`: file protocol, status transitions, context assembly, runner.
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
