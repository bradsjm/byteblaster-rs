# AGENTS.md

Agent guide for `crates/byteblaster-cli`.
This file defines crate-local expectations for automated coding agents.

## Scope

- Crate: `byteblaster-cli` (binary crate).
- Role: command-line UX, argument parsing, command dispatch, stdout/stderr contract.
- Depends on `byteblaster-core` for protocol/runtime functionality.

## Before You Change Code

- Read root `AGENTS.md` first, then this file.
- Keep CLI behavior changes explicit and test-covered.
- Preserve output contract stability (especially JSON output fields).
- Keep business/protocol logic in core crate, not in CLI command handlers.

## Build, Lint, and Test Commands

Run from repo root.

### Fast crate-focused loop

```bash
cargo build -p byteblaster-cli
cargo test -p byteblaster-cli
```

### Required quality gates for this crate

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p byteblaster-cli
```

### Run a single test (important)

Use test-name filter:

```bash
cargo test -p byteblaster-cli cli_output_channeling
cargo test -p byteblaster-cli cli_stream_json_fixture
```

Use exact match when needed:

```bash
cargo test -p byteblaster-cli cli_output_channeling -- --exact
```

Run integration target:

```bash
cargo test -p byteblaster-cli --test cli_contract
```

Discover tests before selecting one:

```bash
cargo test -p byteblaster-cli -- --list
```

Debug failing tests with captured output:

```bash
cargo test -p byteblaster-cli <test_name> -- --nocapture
```

## Local Run Commands

```bash
cargo run -p byteblaster-cli -- --format json inspect path/to/capture.bin
cargo run -p byteblaster-cli -- --format json stream path/to/capture.bin
cargo run -p byteblaster-cli -- --format json download ./out path/to/capture.bin
```

Live mode examples:

```bash
cargo run -p byteblaster-cli -- --format json stream --email you@example.com --max-events 100
cargo run -p byteblaster-cli -- --format text download ./out --email you@example.com --idle-timeout-secs 30
```

## Crate Architecture Boundaries

- Keep command implementations in `src/cmd/*`.
- Keep CLI argument parsing and command wiring in `src/main.rs`.
- Keep output rendering/utilities in `src/output.rs`.
- Keep serialization and presentation concerns in CLI crate.
- Do not duplicate protocol parsing/runtime logic from `byteblaster-core`.

## Code Style Guidelines

### Formatting and linting

- Always run `cargo fmt --all` before finalizing.
- Keep clippy clean under `-D warnings`.

### Imports

- Prefer explicit imports over wildcard imports.
- Keep imports minimal and local to file/module usage.
- Alias only when needed for readability.

### Types and APIs

- Use `clap` derive patterns consistently for args/subcommands.
- Keep command option names stable unless change is intentional and documented.
- Use strongly typed enums (`ValueEnum`) for user-facing mode choices.

### Naming

- Types/traits/enums: `UpperCamelCase`.
- Functions/modules/variables: `snake_case`.
- Constants: `SCREAMING_SNAKE_CASE`.
- Use names that reflect command semantics (`inspect`, `stream`, `download`).

### Error handling

- Return typed/structured errors from command functions when possible.
- In top-level CLI flow, use `anyhow::Result<()>` for command orchestration.
- Propagate with `?`; avoid `unwrap()` in production paths.
- Use `expect(...)` only inside tests with precise failure messages.

### Output and logging contract

- Command payloads and machine-readable data go to `stdout`.
- Diagnostics/logs/warnings go to `stderr`.
- JSON output must remain deterministic and backwards-stable for clients.
- Keep text output concise and unambiguous.

## Testing Expectations

- Prefer integration tests in `crates/byteblaster-cli/tests/*.rs`.
- Validate stdout/stderr channel behavior for contract-sensitive changes.
- Validate JSON shape/fields when changing output payloads.
- For download behavior, assert filesystem side effects deterministically.
- Keep tests deterministic and independent from external network timing.

## Documentation Requirements

- Update `crates/byteblaster-cli/README.md` when command behavior or flags change.
- If protocol-facing behavior changed via CLI integration, sync with root docs as needed.

## Cursor/Copilot Rules

Repository check status at time of writing:

- `.cursorrules`: not present
- `.cursor/rules/`: not present
- `.github/copilot-instructions.md`: not present

If these are added later, treat them as higher-priority local constraints.
