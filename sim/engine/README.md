# Independent rules prototype

Card facts come from the versioned card database snapshot. Effects come from
`authored/effects/`, validated against `dsl/effects.schema.json` before and after
macro expansion. Missing and partial programs are rejected. Some declared
semantics are not executable yet; see [KNOWN_LIMITS.md](../../KNOWN_LIMITS.md).

`Game` owns resolution, visibility and legal actions. Runner adapters convert
protocol types. `Replay` owns immutable branches; `Assist` owns sandbox and shadow
state. AI accepts a player packet and the public catalogue, with no reference to
the source game.

Run all four shared evaluations with the sealed common input:

```bash
cargo run --locked --release -p sve-engine --bin sve-prototype -- /path/to/cards.jsonl . all target/prototype
```

Modes are `all`, `g1`, `ai`, `replay`, `assist`, `validate` and `rules`. Evaluation output
contains unchanged runner reports, a Q4 audit and two core-written search forests.
Forest identities are hypothetical and do not reuse unseen match IDs.

`all` runs G1 and the three architecture/AI suites. Use `rules` without a selection
to run all 706 public scenarios, or supply the new selection as the final argument:

```bash
cargo run --locked --release -p sve-engine --bin sve-prototype -- /path/to/cards.jsonl . rules target/prototype/new docs/evaluation/seal-2/new-selection.yaml
```

Successful CLI exit means reports were written; inspect their statuses for failures.

Tests use synthetic cards for boundaries and the immutable original snapshot for
the shared suites. Supply it explicitly instead of maintaining another card table:

```bash
export SVE_TEST_SNAPSHOT=/path/to/cards.jsonl
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo llvm-cov --locked --workspace --fail-under-lines 90
```

The G1 regression requires all 41 scenarios to pass every checkpoint. The new
139-scenario regression also requires every scenario to pass without exceptions.
It currently fails one assertion in BP10-050 A2: the engine retains a singleton
stack-recipient input that the fixture omits, following its interpretation of CR
13.3.2.4 and contract 9.8. This difference awaits a common ruling. The standard
coverage command consequently exits 101; no test is skipped or treated as passing.
The complete `--ignore-run-fail`
collection records 93.82% line coverage separately from this test failure.
Pending choices include each legal parameter combination, and resolution events
follow effect completion and removal from the resolution zone. Current results and
reproduction commands are in [RESULTS.md](../../docs/evaluation/seal-3/RESULTS.md).
The full public suite has 697 passes, eight failures and one adapter error across
706 scenarios. The remaining input/event-format questions retain their original
verdicts pending a common ruling; see [PROGRESS.md](../../PROGRESS.md).

`Catalog::from_documents` accepts snapshot and YAML strings without filesystem
access. The default `runner` feature enables evaluation adapters and the CLI. The
library builds for `wasm32-unknown-unknown` with `--no-default-features`; browser
bindings, host time and execution have not been validated.
