# sve-scenario-runner

Engine-neutral runner for the rules scenarios in [`tests/rules-scenarios/`](../../tests/rules-scenarios/).
An engine under test implements the `Engine` trait through a thin adapter; the runner
loads each scenario, submits its decisions in order and compares every checkpoint as
[`CONTRACT.md`](../../tests/rules-scenarios/CONTRACT.md) specifies.

```rust
use sve_scenario_runner::{load_dir, load_selection, score_g1, summary};

let root = "tests/rules-scenarios".as_ref();
let questions = load_dir(&root.join("questions"))?;
let selection = load_selection(&root.join("g1-selection.yaml"))?;
// G1 scoring fails closed: named scenarios only, exactly 41, verified questions only.
let reports = score_g1(&mut my_adapter, &questions, &selection, 41)?;
println!("{:?}", summary(&reports));
```

For the runner's own tests there is also `run(engine, questions, selection, RunOptions)`,
which does not require verified questions by default.

## What an adapter must do

| Method | Contract |
| --- | --- |
| `load(fixture)` | Build the position from the neutral `setup` (after `inherit`), `card_facts` and `random`. Keep the scenario's object ids; name new objects `new-1`, `new-2`… (contract 9.1) |
| `decide(decision)` | Submit one decision as written in the question, advance to the next input point or the end of the game, return the outcome and the events produced on the way |
| `query(view, path)` | Answer an `assert` path from that player's view; unidentified cards in zone lists are bare `{"filler": n}`; `knowledge` returns `{"identifiable": [ids]}` |
| `awaiting(view)` | `{"by": "P1", "choices": [...]}` for the decision the engine waits for, with the **complete** option set; for the other player's view, `{"by": "P1"}` only |
| `projection(view)` | The payload actually sent to that player (or to an AI playing that seat), in any shape |

Answers for a player view must come from that player's projection, not from the
omniscient state: the runner scans the projection and the `awaiting` output for
hidden object ids and for card numbers of hidden cards.

The adapter **converts formats only**. It must not resolve rules, pick among legal
options, or filter hidden information for the engine: those are what is being tested.

Events are contract-shaped objects (`{"kind": "ダメージ", "source": ..., "target": ..., "amount": 2}`).
Simultaneous events carry the same `group` value; the label is the engine's choice.

## How results are reported

| Verdict | Meaning |
| --- | --- |
| `pass` | Every checkpoint matched |
| `fail` | At least one mismatch; each is listed with what was expected and what the engine produced |
| `unsupported` | The engine said it does not implement something the scenario needs |
| `adapter-error` | The adapter failed; kept apart from rule failures |
| `ineligible` | Not scored: with `require_verified`, the question is not `verified` or relies on an unconfirmed card fact |

A selection that names an unknown question or scenario, a duplicate, or a checkpoint
outside its scenario's decisions is an error before anything runs.

## Comparison rules

Where the contract is silent, the runner uses the following:

- **Written keys only**: only keys written in `expected` are compared; fields the engine adds are ignored
- **Zones**: `deck` is ordered; every other zone is a multiset. `{filler: n}` counts as `n` unidentified cards at that position
- **Other lists**: multisets of partially matching items
- **Numbers**: compare by value
- **Events**: subsequence by default; a `group` must fall inside one engine group, in any order
- **`events_exact`**: restricted to the listed kinds, the engine's events equal the listed `events`
- **`forbidden_events`**: no engine event may match any of the patterns
- **Event range**: runs from the previous distinct checkpoint to this one; views at one checkpoint share it
- **Placeholders**: a placeholder must be exactly `{filler: n}`; one carrying an id or any other field is not unidentified
- **`group`**: every group label must be one contiguous block in the engine's event stream, whether or not the question mentions that group
- **`awaiting`**: same player; `choices` equal the complete option set field for field; from the other player's view no `choices` may be sent
- **Hidden information**: no hidden object's id may appear anywhere in the viewer's projection or `awaiting` output. Its card number may not appear either, unless another card with that number is one the viewer can identify
- **YAML**: booleans are YAML 1.2 (`true`/`false` only), so ids such as `y` or `on` stay strings. The question set parses identically with PyYAML

## Tests

`cargo test -p sve-scenario-runner` runs the runner against all 184 questions with two fake engines:

- **`Oracle`**: plays back each scenario's expected results. It must pass all 567 scenarios
- **`Mutant`**: breaks exactly one thing. It must fail every scenario the mutation applies to. The 12 mutations:
  - outcome
  - a dropped event
  - a split group
  - an interleaved group
  - an injected forbidden event
  - a number
  - the awaited player
  - opponent choices
  - leaked knowledge
  - a leak in the projection
  - a placeholder carrying an id
  - deck order

`tests/contract_edges.rs` checks contract edge cases with hand-written scenarios and a scripted engine, independent of any question's `expected`. It covers:

- **Events**: groups, subsequence, repeated events, `events_exact`, forbidden patterns
- **Zones**: zones, placeholders
- **`awaiting`**
- **Event windows**: across decisions
- **Leaks**: by id and by card number
- **Runner results**: `unsupported` and `adapter-error` reported apart; bad checkpoints and selections; ineligible questions
- **`inherit`**

## Known limits

Two checks cannot be done inside the runner. Both must be covered when a real
engine and its adapter are connected:

- **Where the projection comes from**:
  - `projection`, `query` and `awaiting` are three separate answers, so the runner cannot prove they come from the same player payload
  - The adapter must use the engine's actual outgoing serialisation as `projection` and derive the other two from the same snapshot
- **A public copy of the same card**:
  - When the viewer can identify a public card with the same number as a hidden one, a leak of the hidden card's number looks like a mention of the public card
  - The runner cannot tell these apart in a payload of arbitrary shape (`known_limit_public_copy_masks_a_card_number_leak` pins this down)
  - Cover it with paired positions on the real player payload: same public information, different hidden cards, identical output for the viewer
