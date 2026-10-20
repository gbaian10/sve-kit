# Author entry fixtures

Ability content follows the official Japanese examples in
`docs/dsl/author-syntax-1.0.md`: BP08-038 (§13 GAP-A-001), BP09-SL05
(§13 GAP-A-002), ECP01-058 (§13 GAP-A-019), and CP04-060 (§14 GAP-A-006).
The permitted official content retains its original rights; see `LICENSING.md`.

Every provenance container is synthetic: binding/revision/face identifiers,
source hashes, spans, authors and dates are not adopted source records or review
evidence. The bodies represent selected constructs, not complete card programs.
BP09-SL05's first face is deliberately empty; this exercises an allowed empty
container and does not assert that the real face has no abilities. Token definitions
are not attached to the parent card, and no token closure is claimed.

Frozen text evidence can be located in these parsed inputs, including every
document and face rather than only the first observation:

| Card | Input | Face index | Text line ordinal | Exact line SHA-256 |
| --- | --- | --- | --- | --- |
| BP08-038 | `card-data/jp/BP08/000-099.json` | 0 | 1 | `7a884eac36c1801277243b0bd7489062e396130b52be10bf3ab09ea4869904cb` |
| CP04-060 | `card-data/jp/CP04/000-099.json` | 0 | 0 | `86fa086090511c9105cfc1d9210f2102f69ef7351e6930dcd9dd89a8f53f3350` |
| BP09-SL05 | `card-data/jp/BP09/SL/000-099.json` | 1 | 1 | `298430d53c7ea027f74e3175d1d374c5fcd40f7a9fa1bb8e9db53b85cce26b80` |
| ECP01-058 | `card-data/jp/ECP01/000-099.json` | 0 | 0 | `c5b5200342452cde425e90b5db756548f926d3122c1d6e1f118a89fb9bcfc3f8` |

Line hashes cover exact UTF-8 line content without the separating newline.
Parsed face indices are not permanent FaceIds. CP03-029 is not the deck-rule
example; it must not be substituted for ECP01-058 based on an earlier reference.
