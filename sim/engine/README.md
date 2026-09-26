# Independent rules prototype

The card database snapshot supplies card facts. Effects come from the authoritative
JSON Schema in `dsl/` and pack files in `authored/effects/`. The engine validates
both authored syntax and macro expansions. Partial programs fail closed.

`Game` owns rule resolution, visibility and legal actions. The runner adapters only
convert protocol types. `Replay` owns immutable branches; `Assist` owns sandbox and
shadow state. The AI takes a player packet and the public catalogue, with no access
to the source game.

Run the four shared evaluations with the sealed common input:

```bash
cargo run --locked -p sve-engine --bin sve-prototype -- /path/to/cards.jsonl . all target/prototype
```

The optional mode is `all`, `g1`, `ai`, `replay` or `assist`. Output includes the four
original runner reports, a separate Q4 audit and two engine-written search forests.
The search forests use hypothetical identities; they never reuse unseen match IDs.

Tests use synthetic cards for boundary behavior and the original snapshot for the
shared suites. Supply the immutable snapshot explicitly instead of maintaining a
second copy of card facts:

```bash
export SVE_TEST_SNAPSHOT=/path/to/cards.jsonl
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo llvm-cov --locked --workspace --fail-under-lines 90
```

The shared G1 regression test permits the documented token-event ordering mismatch
in one public scenario; the raw evaluator still reports it as a failure. See
`DESIGN.md` and the sealed reports for supported semantics and remaining limits.

`Catalog::from_documents` accepts snapshot and YAML strings without filesystem
access. The default `runner` feature enables the shared evaluation adapters and CLI;
the rules API is also available with `--no-default-features`.
