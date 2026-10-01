# Wording rule policy 145-v1

The maintainer approved these actions on 2026-10-02. The approval receipt records
that decision with day precision. The receipt format was adopted by the
coordinator under the user's authorization; it is not a separate user review of
the serialization format.

The policy and approval JSON files are immutable inputs. Their canonical JSON
hashes, immutable authored revision and exact file bytes must be pinned before
use. Changing a matcher or its finite parameters requires a new policy and
approval, not editing the historical receipt. Policy approval is not a per-card
human review or an adoption-order answer.

All matches compare complete content for the same face and region. Name, class,
type, cost, attack, defense, ordered traits and title must be exact. A null main
effect is never equivalent to an empty effect. Unless explicitly repartitioning,
section count and ordinals must remain unchanged. Original text is preserved.

| Rule | Exact matcher | Action |
| --- | --- | --- |
| wp:eol-v1 | Every changed text/section becomes exact after CRLF to LF only; reject standalone CR, other characters or boundary changes | equivalent |
| wp:layout-space-v1 | Every changed text/section becomes exact after removing Python Unicode whitespace | classify_only |
| wp:punctuation-v1 | Every changed text/section becomes exact after removing only the policy's finite punctuation set | classify_only |
| wp:identified-reminder-v1 | Main unchanged, same section count, all changed ordinals independently identified as reminders | classify_only |
| wp:repartition-v1 | Sections change but main plus ordered section concatenation stays exact | classify_only |
| wp:term-token-v1 | Sections unchanged, one nonempty distinct finite term pair occurs once and its single directed replacement gives the other main text | classify_only |
| wp:draw-plural-v1 | EN only, both complete main texts match `Draw ([1-9]) cards?\.` with the same ASCII digit, sections unchanged | classify_only |

Whitespace and punctuation diagnostics can change word, number or condition
boundaries, so they still require human review. Reminder ordinals and term pairs
are currently empty: no production inventory has been approved for them.

Synthetic examples: `A\r\nB` to `A\nB` may use the EOL rule, but `A\rB` to
`A\nB` cannot. `1 2` to `12`, `A(B)` to `AB`, and `Draw 1 card.` to
`Draw 1 cards.` are diagnostics only. Changing a protected statistic or a
section boundary prevents EOL equivalence. Every differing observation and
nonempty predecessor must be checked independently; all diff ranges must be
covered without overlap. An uncovered difference returns to human review.
