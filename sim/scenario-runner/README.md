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

## Architecture fixtures (`arch`)

Phase D also asks each design for two thin prototypes: **replay** and **assist**.
`sve_scenario_runner::arch` checks both with the same positions and third-party
expectations, in [`tests/architecture-fixtures/`](../../tests/architecture-fixtures/):
`positions.yaml` (contract-format setups) and `checks.yaml` (paths, fixed assertions,
seeds, hidden identities). The design is `spec/design/d-prep/arch-fixture-design.md` (r5).

```rust
use sve_scenario_runner::arch::{ArchOptions, check_assist, check_replay, load_fixtures};

let fixtures = load_fixtures("tests/architecture-fixtures".as_ref())?;
// A fresh instance per call: several assertions need an engine that never saw the game.
let mut replay = || -> Box<dyn ReplayEngine> { Box::new(MyReplayAdapter::new()) };
let mut assist = || -> Box<dyn AssistEngine> { Box::new(MyAssistAdapter::new()) };
let options = ArchOptions::default(); // plus the approved id and skip paths, see below
for report in check_replay(&mut replay, &fixtures, &options)
    .into_iter()
    .chain(check_assist(&mut assist, &fixtures, &options))
{
    println!("{} {:?}", report.id, report.outcome);
}
```

| Id | What it checks |
| --- | --- |
| R0 | The digest tells states apart: along the path, across a hidden card or deck order, across seeds |
| R1 | Save at a pause mid-resolution, restore into a fresh instance, continue: same outcomes, events, digests; the same shuffle and draw as a run that was never saved |
| R2 | A replay branch at N3 has the digest of a fresh replay to N3 |
| R2b | A replay branch taken after the game went on marks as carried what each player saw after the branch point, and no more; the identities stay known after the shuffle |
| R3 | Branching does not change the source line; node ids stay put and are unique per branch |
| R4 | Undo carries seen identities for both players, is not a decision, is logged, and keeps the original line playable |
| R5 | No hidden id, hidden card number or seed in any player output (projection, awaiting, knowledge, export); paired positions give equal player exports (hidden card, deck order, shuffle result, also on the replay branch: a carried identity is not a carried position) |
| R6 | Events trace their `cause` back to the deciding node through the ability |
| A1–A3 | A skipped trigger is recorded as a divergence, automation halts, Rewind realigns and play continues; after Rewind each step is compared on its own, in both layers, with the fixed assertions at the nodes it reaches |
| A4 | Adopt keeps the pending trigger |
| B1–B3 | A manual value override is recorded; Rewind and Adopt give different, correct follow-ups |

Each result is `pass`, `fail` (with reasons), `unsupported` or `adapter-error`, as in the runner.

### Adapter contract for `arch`

- `advance` returns `None` when it refuses (it must, while a divergence is unresolved)
- `admin_log` entries carry `kind: undo` or `kind: branch`
- `query` must answer the six board paths R4 compares (`P1/P2.hand_count`, `deck_count`, `field`); a missing answer fails R4
- Events carry `id` and `cause`: `{event: <id>}`, `{decision: <node id>}` or `{rule: "<clause>"}`
- Divergences are `{id, kind, expected, actual, refs, resolved}`; only `resolved` may change
- Node and event ids may be random; paired comparisons relabel them by the order the checks
  received them. An id the checks never received keeps its raw value, so it is never
  folded together with another one. Node ids inside player payloads are only relabelled at paths listed in
  `ArchOptions::node_id_paths`; other engine identifiers (transport counters and the like)
  go in `ArchOptions::skip_paths`. **Both lists are approved by the third party before the
  run** and reported with the results; the leak scan always sees the raw payload

### Tests of the checks

`tests/arch.rs` runs scripted engines (`tests/arch_support/`): a correct skeleton that
passes every assertion, and 45 broken skeletons (32 replay, 13 assist), each caught by the
assertion meant for it. Some of them:

| Mutation | Caught by |
| --- | --- |
| Save keeps the board but not the seed; saving consumes randomness; the restored copy drops or miswires an event | R1 |
| Branch shares mutable state with its source; branching touches the source line; ids reused across branches | R3 |
| Constant digest | R0 |
| Digest without carried knowledge | R2b, R4 |
| Replay branch does not carry what P1 saw, hands P2 more, or forgets after the shuffle | R2b |
| Undo forgets what was seen / the opponent's side, deletes or rewrites the original line; board queries unanswered | R4 |
| Export leaks the seed, the shuffled order (also on a replay branch), the opponent's hidden cards, the deck before the search (ids or a hash of the order); knowledge carries a hidden card; a never-issued id encodes the top card | R5 |
| Broken `cause` link | R6 |
| After Rewind a card enters a step early, or a trigger fires twice | A3 |
| Rewind drops or rewrites the divergence | A3, B2 |
| Automation advances while diverged | A2 |
| Adopt ignored / Adopt clears the pending trigger | A4, B3 / A4 |

## AI positions (`ai`)

The shared AI positions of phase D live in [`tests/ai-positions/`](../../tests/ai-positions/)
(see its README for the file format, budget and horizon). An AI prototype is wrapped in
`ai::AiEngine`: `load`, `decide`, `legal`, `projection`, `query` and `think`.

```rust
use sve_scenario_runner::ai::{check_ai, load_positions};

let positions = load_positions("tests/ai-positions".as_ref())?;
let mut factory = || -> Box<dyn AiEngine> { Box::new(MyAi::new()) };
let reports = check_ai(&mut factory, &positions, "my-design/profiles.yaml".as_ref());
```

- Every check starts from a freshly loaded instance; branches replay their parent's decisions, then their own, with the parent's `random` followed by theirs
- `load` gets the position's `room` as `setup.room`, and the seed `ai::LOAD_SEED`
- Legal sets compare as multisets; lists of chosen targets and selected objects compare as sets
- Every `think` must return a legal decision within its budget
- Hard checks report `pass` or `fail`; Q3 and R4 are `diagnostic` only (the AI's self-reported trace, and whether the choice after the reveal changed)
- Q4 (search evidence) and P3 (profile weights) are audits at hand-in. `ai::check_search_log` runs the Q4 check on an engine-written search log

`tests/ai.rs` validates the checks with a scripted AI: it passes every hard check, and
each of twelve broken behaviours is caught by the check meant for it (incomplete or
padded legal sets, stale choices, illegal decisions, peeking at the hidden hand or deck
order, leaking revealed cards, profiles reversed or ignored, `think` changing the
position, a missing public deck list, a leaked hand).
