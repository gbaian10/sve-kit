# Engine integration tests

Run the public-input wrapper from the repository root:

```sh
python3 .github/ci/rust_tests.py
```

New current-card scenarios belong under `current_repository/runtime/`. Use its
`game(&initial)` helper so every test in the binary shares the immutable public
inputs and accepted selection. Build a fresh initial position for each game.
Use `selection_with_rules()` for scenarios that require engine rule bindings;
clone the returned selection when constructing a game.

Tests that change source evidence or catalog contents must clone `inputs()` before
editing and build their changed selection independently. Keep pointer-identity
rejection tests independent too: a fresh equivalent catalog is intentionally a
different accepted selection. Never put mutable game state in a shared fixture.

Pure lowering tests reuse `lowerer()` in their test module. The current-selection
and historical-regression catalogs remain separate fixtures: restoring historical
prototype programs must never change the current-card selection.
