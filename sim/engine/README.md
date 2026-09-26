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

Modes are `all`, `g1`, `ai`, `replay`, `assist` and `validate`. Evaluation output
contains unchanged runner reports, a Q4 audit and two core-written search forests.
Forest identities are hypothetical and do not reuse unseen match IDs.

Tests use synthetic cards for boundaries and the immutable original snapshot for
the shared suites. Supply it explicitly instead of maintaining another card table:

```bash
export SVE_TEST_SNAPSHOT=/path/to/cards.jsonl
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo llvm-cov --locked --workspace --fail-under-lines 90
```

The G1 regression test permits the documented event-order difference in one public
scenario. The raw evaluator still reports that scenario as a failure. Results and
reproduction commands are in [EVALUATION.md](../../docs/evaluation/EVALUATION.md).

`Catalog::from_documents` accepts snapshot and YAML strings without filesystem
access. The default `runner` feature enables evaluation adapters and the CLI. The
library builds for `wasm32-unknown-unknown` with `--no-default-features`; browser
bindings, host time and execution have not been validated.
