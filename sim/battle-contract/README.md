# sim/battle-contract

Wire types of the [battle contract](../../docs/sim/battle-contract.md): the messages between the battle
host (a Web Worker today, a server later) and its clients. The crate holds data types only and does
not depend on the engine; projecting a game into these types belongs to the engine integration.

Rust is the single source. The JSON Schema comes from these types, and the TypeScript types and the
validator in `sim/web` come from that schema; nothing generated is committed.

```bash
cargo run --locked -p sve-battle-contract -- schema   # print the JSON Schema
cd sim/web && bun run generate:contract               # write the TypeScript types and the validator
cd sim/web && bun run check:contract                  # reproducible generation and the round trip
```

- `encode` and `decode` are the boundary: `decode` accepts only text that states a message exactly
  (every field present, no unknown field, no repeated key, every number a safe integer) and reports
  any other input as a `HostError`.
- `Nat` and `Int` keep numbers within what a JavaScript `number` holds exactly; conversions from raw
  Rust values are checked.
- `fixtures/` holds hand-written, synthetic messages. The Rust tests decode them and damage them field
  by field; the `sim/web` tests run the generated validator over the same files; `check:contract`
  sends each one from Rust through the validator and back.
