# AI positions

Shared positions for the D-stage AI prototypes: same position, same visible
information, same budget for both designs. Positions use the setup format of
[`../rules-scenarios/CONTRACT.md`](../rules-scenarios/CONTRACT.md) v2.1, plus the
fields below.

| File | Position | What it checks |
| --- | --- | --- |
| `h.yaml` | AI-H | The legal set is exactly the one derived by hand, at the root and at the opponent's later choices |
| `q-a.yaml`, `q-b.yaml` | AI-Q | The opponent may hold a Quick card; the AI's choice may not depend on which card P2 actually holds |
| `r-a.yaml`, `r-b.yaml` | AI-R | A choice after private cards are revealed mid-resolution; before the reveal the AI may not depend on the deck order |
| `p.yaml` | AI-P | Two deck profiles lead to different, predictable attacks |

## Extra fields

- `room: {open_decklists: true}`: a room where the deck lists given in the position are public
- `players.<P>.deck_list`: that player's public deck list, as `{card, count}`. With `open_decklists`, every `deck_list` given in the file is part of **both** players' projections; a player without `deck_list` has no public list
- `pair: <file>`: the other position of a pair. Paired positions differ only in hidden information

## Checks

| Key | Meaning |
| --- | --- |
| `before` | Decisions to submit first, in contract format; or the id of a check whose `before` to reuse |
| `branches` | Checks that continue from this check's state: each branch starts from a copy of the parent's state after the parent's `before`, then submits its own `before`. Siblings start from the same parent state |
| `random` (in a branch) | Controlled results for randomness that happens while submitting this branch's `before`, in the format of contract section 5, applied after the parent's |
| `query` / `equals_setup` | The value at that path from that view equals the given part of the setup |
| `legal_exact` | The engine's complete legal set at that point equals this list (distributions compare as maps) |
| `think` | Ask the AI of `seat` for a decision with this profile, budget and seed |
| `decision_in` | The AI's decision belongs to that check's `legal_exact` |
| `equal_to_pair` | These fields of the AI report equal the paired position's |
| `projection_equal_to_pair` | That seat's projection equals the paired position's |
| `projection_hides_from` / `objects` | That seat's projection contains none of these object ids or their card numbers |
| `same_instance` | Several `think` calls on one engine instance, each with its expected decision |
| `unchanged: omniscient` | The omniscient projection is the same before and after **each** `think` call |

## Randomness

- `random` in a file (and in branches) controls only the decisions this file submits, as in the question contract
- Randomness inside the AI's search (sampling hidden cards, simulating shuffles) belongs to the AI's hypothetical samples and is derived from the `think` seed. It never consumes the file's `random`

## Profiles

Each design ships `profiles.yaml` listing three profiles, all with `subtype` and `period`:

```yaml
- {id: general, subtype: general, period: 2026-09, file: <path>}
- {id: aggro, subtype: face, period: 2026-09, file: <path>}
- {id: control, subtype: board, period: 2026-09, file: <path>}
```

## Budget and horizon

- Budget: 5,000 explored **player-decision edges** per `think` call: applying one legal decision at any input point counts 1, for either player, including Quick passes, trigger ordering and choices during resolution. Automatic resolution between two input points does not count. Every determinization sample counts its own edges. Report engine-internal steps, sample counts and time separately
- Horizon: until the end of the current turn, including the non-turn player's Quick timings (8.4.7, 7.4.5). Cards that can be seen within the horizon are concrete; the rest may be `filler`
