# Legacy random vectors

These synthetic fixtures were captured by the engine at main commit
`1db57587377b72e5ca21f220a25e537cd4101ecb`, before versioned RNG integration.
They contain only the invented `unit` and `spell` cards from
`tests/support/random_fixtures.rs` and fixed structural card-type labels.

With seed `legacy-vector`, create the game and start replay `legacy`, play `s`,
then save at the optional prompt. `legacy-save.json` is that original
`astra-save/1` blob, including numeric RNG state. Resume with `execute` to obtain
the events, final referee packet and digest in `legacy-expected.json`. The first
play exercises a shuffle, random selection and die; the continuation rolls again.

The tests compare events, digests, projection and the original save bytes
(excluding an optional trailing newline) without regenerating expectations from
the new implementation. There are no official card names or effect text.
