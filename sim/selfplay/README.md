# sve-selfplay

Random self-play over the local battle host. Both seats are wire players: every decision is a
battle-contract answer drawn at random from the domain the question states, so a match takes
the same projection, decoding and rules path as the browser Worker. It looks for matches
that stop without a rules result, for broken invariants and for slow submissions.

A seed names the two decks, the first chooser and every answer. Decks follow base
construction only (40 to 50 main cards and up to 10 evolve cards of one class plus neutral,
or of one title; at most three cards per rules name); formats and ban lists are ignored so
restricted cards stay exercised. Tokens enter through the cards that create them.

## Running

The tool reads the battle inputs of a verified public export, the same ones the Worker loads.
From the repository root:

```bash
uv --directory carddb run --locked sve-carddb snapshot battle-inputs \
  --export-dir <export>/public --region jp \
  --engine-rules "$PWD/authored/rules/engine/index.yaml" --output <work>/reader --published
cargo run --locked --release -p sve-selfplay -- prepare <work>/reader <export>/public <work>
cargo run --locked --release -p sve-selfplay -- \
  run <work>/inputs.json <work>/programs.json <work>/run-1 0 3000 6 40
```

`run` takes the first seed, the number of matches, the worker threads (default 4) and how
many of the first seeds also keep their record (default 0). It writes:

| File             | Content                                                                                                                                      |
| ---------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| `summary.json`   | Outcomes, failure groups with seeds and cards, receipt counts, cost percentiles, the 20 slowest submissions, cards never built or never seen |
| `failures.jsonl` | One line per failed match: the failure, both seats' last updates and the record                                                              |
| `records.jsonl`  | Records of the first seeds, for timing replays                                                                                               |
| `excluded.json`  | Printings with a program that failed lowering or admission and so entered no deck, or no deck of their title                                 |

`replay <inputs.json> <programs.json> <records.jsonl> <timings.jsonl>` submits each record's
commands again and writes the cost of every submission. A record holds the host's setup and
every command, so it replays without this crate's generators; a receipt that differs from
the record is a `replay` failure.

## What counts as a failure

| Kind             | Meaning                                                                                     |
| ---------------- | ------------------------------------------------------------------------------------------- |
| `engine_fault`   | The authority faulted; the detail is its own diagnostic                                     |
| `host_error`     | The host stopped, usually a stop it could not project; the detail names the native question |
| `invariant`      | A check below failed                                                                        |
| `no_answer`      | No answer satisfies the question's stated domain (`proven` when the whole domain was tried) |
| `rejected`       | The host refused four answers in a row that the stated domain allows                        |
| `decision_limit` | No result after 3,000 submissions                                                           |
| `event_limit`    | One submission committed more than 2,000 events for a seat                                  |
| `load`           | The decks did not open                                                                      |
| `panic`          | The host panicked                                                                           |

Both limits count; neither is a wall-clock limit. A submission that never returns cannot be
counted from outside the engine and shows as a run that stops making progress.

Invariants, read from the two seats' updates alone:

- every update survives the contract's strict decoder unchanged, and its sequence never goes back
- zone members exist in the object table exactly once; no zone exceeds its capacity
- exactly one seat is asked, or both see the same result
- at a settled stop (a main, Quick or resolution question, or the end) both seats see the
  same turn, resources, zone sizes and public cards; while a seat is still declaring a play
  its own view runs ahead by design
- deck cards never increase: zone sizes minus generated objects stay at or below the last count
- a seat is told a printing only the other deck contains only after it was shown: public in
  the owner's view, named by an event both seats received, or in the seat's own notices

## Costs

Every submission records the host time to the next stable stop, the time to serialize the
resulting messages and the committed events it produced. The summary gives p50, p95, p99 and
the maximum, and the slowest submissions with their question, cards and board size.

To time the WebAssembly build on the same records:

```bash
cargo build --locked --profile wasm --target wasm32-unknown-unknown -p sve-battle-host
wasm-bindgen --target nodejs --out-dir <work>/wasm \
  target/wasm32-unknown-unknown/wasm/sve_battle_host.wasm
node sim/selfplay/replay-wasm.mjs <work>/wasm/sve_battle_host.js \
  <work>/inputs.json <work>/programs.json <work>/run-1/records.jsonl <work>/wasm-timings.jsonl
```

The WebAssembly call includes decoding the command and encoding the messages, so compare it
with `engine_micros + encode_micros` of the native replay.

## Tests

`cargo test -p sve-selfplay` plays a synthetic export end to end and, on the public card
inputs that `.github/ci/rust_tests.py` generates, two fixed seeds for 30 submissions each.
Large runs are a local job.
