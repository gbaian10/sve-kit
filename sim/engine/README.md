# Independent rules prototype

Card facts come from the versioned card database snapshot. The engine has two
effect paths sharing one `Game` and authority state:

- The typed path reads the complete accepted author selection from
  `authored/rules/author-effects/` and named `effect-bodies/`. DSL Schemas, the
  Python reader, Rust `ExpandedLowerer`, typed IR and runtime admission exist;
  syntax validation alone does not establish execution support.
  `Catalog::from_current_selection` and `Game::from_selected_effects` admit current
  whole cards. See [the author contract](../../docs/dsl/author-contract-1.0.md).
- The frozen prototype reads `authored/rules/effects/`, validated against
  `dsl/effects.schema.json` before and after macro expansion. Only engine tests
  still drive this path, through the old `decide`/`legal` and player packet.

The shared rule exam runs on the typed path: `TypedAdapter` plays both seats
through the player interface only, and `sim/engine/tests/typed_exam.rs` compares the
result with [tests/engine/typed-known-results.yaml](../../tests/engine/typed-known-results.yaml).
See [engine status](../../docs/sim/engine-status.md#驗證方式) for the list rules.

The player match interface is being built on the typed path. The old
`decide`/`legal`/`projection` and JSON interpreter are frozen test drivers. The
prototype replay, assist and AI were removed; their typed successors are tracked in
[#2206](https://github.com/gbaian10/sve-kit/issues/2206),
[#2207](https://github.com/gbaian10/sve-kit/issues/2207) and
[#2208](https://github.com/gbaian10/sve-kit/issues/2208). Draw, movement,
combat, turn and terminal primitives still called by typed execution remain
maintained. The first typed AI uses a basic policy through the player interface;
search is a separate follow-up. Scope and progress are tracked in
[#1971](https://github.com/gbaian10/sve-kit/issues/1971); enduring boundaries are in
[engine status](../../docs/sim/engine-status.md).

Tests use synthetic cards for boundaries and a fresh projection of the pinned JP
batch for the shared suites. The wrapper generates that input, replaces any inherited
`SVE_TEST_SNAPSHOT`, runs the complete workspace, and deletes the temporary file.
Project and fork CI run the same tests with the same 90% line coverage gate;
no private checkout is required. Direct Cargo card tests fail when generated input
is missing or has the wrong pin; they never return early or skip.

```bash
cargo clippy --locked --workspace --all-targets -- -D warnings
python3 .github/ci/rust_tests.py
```

Mutation tests use the same wrapper through the manual `cargo-mutants` hook.

Pending choices include each legal parameter combination, and resolution events
follow effect completion and removal from the resolution zone. Remaining engine
errors and open questions are tracked in
[the engine tracking issue](https://github.com/gbaian10/sve-kit/issues/478).

Effect documents use `format: 1, kind: effect_set`; keywords use
`format: 1, kind: keyword_registry`, selecting the astra/1 prototype grammar.
The private background records each document kind/format and validation Schema
`schema_id`. This migration rebuilds authored/settings/schema fingerprints and
does not accept former backgrounds or replay fingerprints.

`Catalog::load` reads `authored/rules/engine/index.yaml` and verifies its exact
snapshot SHA-256. Its default identity boundary is explicitly `legacy-jp`.
`Catalog::load_with_identity` and `Catalog::from_documents_with_rules` accept an
explicit `EngineIdentityInput`; the latter uses only the supplied bytes. A
`Resolved` projection supplies existing face/rules-name IDs, region and snapshot
hash from that same input. It does not infer JP/EN counterparts or invent IDs.
See [engine rule identities](../../docs/dsl/engine-rules-1.md) for the contract.

Player setup may provide `title_code`. Frozen runner title labels resolve through
the validated anchor index derived from the same generated JP input; a supplied code and label must agree. Empty
capabilities are registered explicitly; unknown titles fail closed. Capabilities
apply only to title construction. Resource costs match current effective names,
including valid aliases, against resolved roles. The EX-to-banish counter uses
only the single rules name after movement, without aliases.

`Catalog::from_documents` remains a filesystem-free entry for generic synthetic
positions, without special bindings. A rule that requires an unbound role returns
`Unsupported`. Bound loaders reject missing roles/templates and inconsistent
references before execution. The resolved input background and required player
codes are saved with the serialized `Game`; missing fields fail
rather than restoring empty defaults. Restore uses that saved background. This
does not provide cross-version migration, `save/2`, or server input authorization.

`Catalog::from_current_selection` consumes the complete accepted author selection.
`Game::from_selected_effects` resolves its permanent face identities against the
snapshot's source text hashes and admits every current whole card before returning
a game. Replacing the prototype catalogue or any candidate after selection
rejects the whole construction. Missing, ambiguous or unsupported cards reject the entire construction;
current cards never acquire prototype programs. Source proof stays inside the
authority, including after departure. Loaded internal cards still reject saves;
this entry does not publish capabilities or grant use qualification.

The default `runner` feature enables the evaluation adapters. The
library builds for `wasm32-unknown-unknown` with `--no-default-features`; browser
bindings, host time and execution have not been validated.

New games use `chacha12-sha256-rand09-v1`:
`ChaCha12Rng` from rand_chacha 0.10.0, with rand 0.10.3's unbiased range sampler
and slice shuffle. These versions are pinned because sampling is part of the
authoritative sequence. The seed is SHA-256 of the bytes
`sve-engine/chacha12-sha256-rand09-v1\0` followed by the exact UTF-8 seed string;
the stream and word position start at zero. Future sequence changes need a new
algorithm name and must retain readers for previous algorithms.

Trusted saves retain the algorithm, seed, stream and complete word position.
The 68-bit word position is encoded as `[low_u64, high_u64]` so JSON round trips
do not truncate it. Player packets contain none of this private RNG state.
Scripted random outcomes still take precedence and do not consume that stream.

RNG objects must include the known algorithm name; numeric states and objects
without an algorithm are rejected. No legacy RNG or seed reconstruction path is
retained: the handwritten generator was never deployed and has no real saves.
Trusted `Game` serialization is unchanged; this does not add
general save migration or untrusted-input authorization.
