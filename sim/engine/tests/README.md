# Engine integration tests

Run the public-input wrapper from the repository root:

```sh
python3 .github/ci/rust_tests.py
```

New current-card scenarios belong under `current_repository/runtime/`. Use its
`game(&initial)` helper so every test in the binary shares the immutable public
inputs and accepted selection. Build a fresh initial position for each game.
Games share the immutable runtime definitions prepared for that accepted selection; their positions, decisions and
ability uses remain independent. Additional `load_internal_card()` calls affect
only that game.
Use `selection_with_rules()` for scenarios that require engine rule bindings;
clone the returned selection when constructing a game.

Tests that change source evidence or catalog contents must clone `inputs()` before
editing and build their changed selection independently. Keep pointer-identity
rejection tests independent too: a fresh equivalent catalog is intentionally a
different accepted selection. Never put mutable game state in a shared fixture.

A helper with unchanged custom inputs should keep its `SelectedEffects` in a
`OnceLock` and clone that selection for each initial position. Different source
mutations require independent selections. Do not build a complete selection
inside a parameter loop when only the position changes.

Pure lowering tests reuse `lowerer()` in their test module. The current-selection
and historical-regression catalogs remain separate fixtures: restoring historical
prototype programs must never change the current-card selection.
