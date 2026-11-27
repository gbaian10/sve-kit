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
many of the first seeds also keep their record (default 0). `--decks <decks.json>` before the
other arguments plays listed decks instead of random ones (see below). It writes:

| File                 | Content                                                                                                                                       |
| -------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| `summary.json`       | Outcomes, failure groups with seeds and cards, receipt counts, cost percentiles, the 20 slowest submissions, cards never built or never seen  |
| `failures.jsonl`     | One line per failed match: the failure, both seats' last updates and the record                                                               |
| `records.jsonl`      | Records of the first seeds, for timing replays                                                                                                |
| `excluded.json`      | Printings with a program excluded from random construction, or from random construction for their title                                       |
| `cards.json`         | Per card number: matches where it was in either deck or publicly visible at an update, event counts and failed matches with it in either deck |
| `refused-decks.json` | With `--decks`: listed decks left out, with the reason                                                                                        |

### Deck lists

A deck list names each deck by card number and count, with a relative integer weight:

```json
{
  "decks": [
    {
      "label": "a",
      "weight": 3,
      "construction": { "kind": "class", "value": "ロイヤル" },
      "main": { "BP01-001": 3, "…": 1 },
      "evolve": { "BP01-003": 2 }
    }
  ]
}
```

`construction` is a printed class or a registered title code; `label` and `evolve` are
optional. Each match draws both seats' decks independently by weight, so a heavy deck also
meets itself. The tool checks base construction, whole-deck admission and host startup
in a mirror. In a mixed list it also checks each class deck against an admitted title deck
because control transfers can require additional Drive icon facts. It writes skipped decks
and their reasons to `refused-decks.json` before returning an error for an all-refused list.
The summary's `lineup` gives each admitted deck's seats, results and mirror matches.

`cards.json` counts, per card number, the played cards, the ability plays and the ability
resolutions from its text, the moves onto a field or into a cemetery, and the evolutions that
placed it. Counts use identities the host disclosed in committed events. The tool takes the
larger count from the two feeds for each printing, event kind and playing or destination seat
where available. The host sends no pending-trigger events, so a triggered ability is counted
only in `resolved`, once it resolves. `built` and `failed` count a card at most once per match.

`replay <inputs.json> <programs.json> <records.jsonl> <timings.jsonl>` submits each record's
commands again and writes the cost of every submission. A record holds the host's setup and
every command, so it replays without this crate's generators. Lines of `failures.jsonl` are
accepted as they are. A receipt that differs from the record, or a match that ends while the
record still has commands, is a `replay` failure and fails the command. A record that stops
before the match does replays to `record_end`: the `no_answer` matches stop that way,
because the question that had no answer was never submitted.

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
| `replay`         | Only in a replay: the match no longer follows the record                                    |
| `record_end`     | Only in a replay: the record has no command for the open question                           |

Both limits count; neither is a wall-clock limit. A submission that never returns cannot be
counted from outside the engine and shows as a run that stops making progress.

Invariants, read from the two seats' updates alone:

- every update survives the contract's strict decoder unchanged, and its sequence always grows
- zone members exist in the object table exactly once; no zone exceeds its capacity
- exactly one seat is asked, or both see the same result
- both seats see the same turn, resources, zone sizes and public cards, except while a seat
  is declaring, targeting, allocating or paying for a play: the host shows a play to the
  other seat only once it is played
- deck cards never increase: zone sizes minus generated objects stay at or below the last count
- a seat is told a printing only the other deck contains only after it was shown: public in
  the owner's view, moved through a public zone by an event of the owner's own feed, or in
  the seat's own notices. A movement between hidden zones shows nothing

## Costs

Every submission, the one that ended the match included, records the host time to the next
stable stop, the time to serialize the resulting messages and the committed events it produced.
The summary gives p50, p95, p99 and the maximum, and the slowest submissions with their
question, cards and board size. A submission that panics inside the host is the exception:
it has no cost and its recorded receipt is a placeholder.

To time the WebAssembly build on the same records:

```bash
cargo build --locked --profile wasm --target wasm32-unknown-unknown -p sve-battle-host
wasm-bindgen --target nodejs --out-dir <work>/wasm \
  target/wasm32-unknown-unknown/wasm/sve_battle_host.wasm
node sim/selfplay/replay-wasm.mjs <work>/wasm/sve_battle_host.js \
  <work>/inputs.json <work>/programs.json <work>/run-1/records.jsonl <work>/wasm-timings.jsonl
```

The WebAssembly call includes decoding the command and encoding the messages, so compare it
with `engine_micros + encode_micros` of the native replay. Its `end` is `finished`,
`engine_fault`, `host_error: <reason>`, `load: <stage>`, `replay: …` for a receipt that
differs (the script then exits with 1), `panic: …` for a trap (it stops there), or
`unfinished` for every other way a recorded match stopped short.

## Tests

`python3 .github/ci/rust_tests.py test --locked -p sve-selfplay` plays a synthetic export end
to end and, on the public card inputs the wrapper generates, two fixed seeds for 30
submissions each. Large runs are a local job.
