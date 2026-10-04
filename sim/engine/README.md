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

The positional interface is `sve-prototype SNAPSHOT [ROOT] [MODE] [OUTPUT]
[SELECTION_OR_KNOWN]`. `SNAPSHOT` is required; `ROOT`, `MODE` and `OUTPUT` default
to `.`, `all` and `target/prototype`. The final path is used by `rules` and `gate`.
Use `--help` or `--version` without a snapshot. Invalid modes, unknown options and
excess arguments exit with status 2 before input loading or report creation.
Put `--` before positional arguments if a path starts with a dash.

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

`Catalog::load` reads `authored/engine-rules/index.yaml` and verifies its exact
snapshot SHA-256. Its default identity boundary is explicitly `legacy-jp`.
`Catalog::load_with_identity` and `Catalog::from_documents_with_rules` accept an
explicit `EngineIdentityInput`; the latter uses only the supplied bytes. A
`Resolved` projection supplies existing face/rules-name IDs, region and snapshot
hash from that same input. It does not infer JP/EN counterparts or invent IDs.
See [engine rule identities](../../docs/dsl/engine-rules-1.md) for the contract.

Player setup may provide `title_code`. Frozen runner title labels resolve through
the validated private anchor index; a supplied code and label must agree. Empty
capabilities are registered explicitly; unknown titles fail closed. Capabilities
apply only to title construction. Resource costs match current effective names,
including valid aliases, against resolved roles. The EX-to-banish counter uses
only the single rules name after movement, without aliases.

`Catalog::from_documents` remains a filesystem-free entry for generic synthetic
positions, without special bindings. A rule that requires an unbound role returns
`Unsupported`. Bound loaders reject missing roles/templates and inconsistent
references before execution. The resolved input background and required player
codes are saved inside the existing prototype `astra-save/1`; missing fields fail
rather than restoring empty defaults. Restore uses that saved background. This
does not provide cross-version migration, `save/2`, or server input authorization.

The default `runner` feature enables evaluation adapters and the CLI. The
library builds for `wasm32-unknown-unknown` with `--no-default-features`; browser
bindings, host time and execution have not been validated.
