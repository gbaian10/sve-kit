# Independent rules prototype

Card facts come from the versioned card database snapshot. Effects come from
`authored/effects/`, validated against `dsl/effects.schema.json` before and after
macro expansion. Missing and partial programs are rejected. Some declared
semantics are not executable yet; see
[docs/sim/engine-status.md](../../docs/sim/engine-status.md).

`Game` owns resolution, visibility and legal actions. Runner adapters convert
protocol types. `Replay` owns immutable branches; `Assist` owns sandbox and shadow
state. AI accepts a player packet and the public catalogue, with no reference to
the source game.

Run all four shared evaluations with the sealed common input:

```bash
cargo run --locked --release -p sve-engine --bin sve-prototype -- /path/to/cards.jsonl . all target/prototype
```

Modes are `all`, `g1`, `ai`, `replay`, `assist`, `validate`, `rules` and `gate`. Evaluation output
contains unchanged runner reports, a Q4 audit and two core-written search forests.
Forest identities are hypothetical and do not reuse unseen match IDs.

`all` runs G1 and the three architecture/AI suites. Use `rules` without a selection
to run all 706 public scenarios, or supply a selection file as the final argument,
for example the G1 selection:

```bash
cargo run --locked --release -p sve-engine --bin sve-prototype -- /path/to/cards.jsonl . rules target/prototype/g1 tests/rules-scenarios/g1-selection.yaml
```

`gate` runs every public scenario and fails unless the result matches
[docs/m0/known-failures.yaml](../../docs/m0/known-failures.yaml) exactly.

Successful CLI exit means reports were written; inspect their statuses for failures.

Tests use synthetic cards for boundaries and the immutable original snapshot for
the shared suites. Supply it explicitly instead of maintaining another card table:

```bash
export SVE_TEST_SNAPSHOT=/path/to/cards.jsonl
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo llvm-cov --locked --workspace --fail-under-lines 90
```

The G1 regression requires all 41 scenarios to pass every checkpoint. The gate
test requires all 706 public scenarios to pass or be listed in
`docs/m0/known-failures.yaml`, which is currently empty. Pending choices include
each legal parameter combination, and resolution events follow effect completion
and removal from the resolution zone. Remaining engine errors and open questions
are tracked in [docs/m0/known-errors.md](../../docs/m0/known-errors.md).

`Catalog::from_documents` accepts snapshot and YAML strings without filesystem
access. The default `runner` feature enables evaluation adapters and the CLI. The
library builds for `wasm32-unknown-unknown` with `--no-default-features`; browser
bindings, host time and execution have not been validated.
