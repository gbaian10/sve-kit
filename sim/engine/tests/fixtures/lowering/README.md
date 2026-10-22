# Candidate lowering fixture

BP01-118 蠢く死霊 encodes its fanfare: all leaders take 1 damage in one
instruction. The Japanese source is the reviewed
`card-data/jp/BP01/100-199.json`, face 0, text line 0, from the
[official card page](https://shadowverse-evolve.com/cardlist/?cardno=BP01-118).
The source coordinates and metadata in the YAML are synthetic. They provide no
source admission or review evidence; the expansion kernel has `source_proof=none`.
Official content remains outside the project licenses; see `LICENSING.md`.

BP08-038 uses the existing `carddb/tests/fixtures/author_entries/golden.yaml`
fixture and its documented source. A shared test helper calls Python's actual
`AuthorExpander`, passes the selected expanded trees and separate source-proof diagnostics to Rust, and never
stores generated JSON or sidecars. These tests do not admit cards for production.

## Per-card diagnostic entry

After changing these YAML fixtures, lowering or D1 admission, run:

```bash
python3 .github/ci/rust_tests.py test --locked -p sve-engine --test internal_ir author_fixture_admission_report -- --nocapture
```

The complete Rust CI scope also runs this test. It expands this batch with C0,
lowers each card separately and checks the original candidate through D1's
whole-card loader. Printed Spell costs come from the generated snapshot.
The JSON report separates runtime support from admission: every fixture is
`rejected`, with current-source, snapshot-identity, complete-ability and use-qualification
blockers. In particular, BP01-041's Amulet activation is also rejected by D1;
SD03-005's supported draw body does not supply its missing Quick ability.

This is a test diagnostic entry, without a production acceptance branch.
C0's source-checked expansion alone does not supply C1 use qualification or
prove the complete printed ability set. Ordinary builds do not run this entry
or create validation results; they must consume verified inputs as specified in
`docs/dsl/author-contract-1.0.md` §8.

`numeric.yaml` contains two complete ability fragments: BP08-067
マリオネットドラゴン's own end-phase destruction (face 0, text line 1), and
BP18-018 怒れる山神's attack-time +3/+3 (face 0, text line 0).
Their sources are `card-data/jp/BP08/000-099.json` and
`card-data/jp/BP18/000-099.json`, respectively, from the
[BP08-067 official page](https://shadowverse-evolve.com/cardlist/?cardno=BP08-067)
and [BP18-018 official page](https://shadowverse-evolve.com/cardlist/?cardno=BP18-018).
Other printed abilities are outside these fragments. The synthetic source
coordinates have the same candidate-only limitations as the fixtures above.

`search.yaml` contains BP03-002 コスモスファング's complete fanfare
fragment (face 0, text line 1), from `card-data/jp/BP03/000-099.json` and the
[official card page](https://shadowverse-evolve.com/cardlist/?cardno=BP03-002).
Its optional field destination follows Q445 recorded alongside that source.
The evolution ability is outside this fragment; source coordinates are synthetic.

`activated.yaml` contains the third-line activated ability of BP01-041
騎士王の威光 (3 PP, Act, draw one) and BP11-099 リボルバーイーグル
(6 PP, +4/+4). Sources: `card-data/jp/BP01/000-099.json` and
`card-data/jp/BP11/000-099.json`, from the
[BP01-041 official page](https://shadowverse-evolve.com/cardlist/?cardno=BP01-041)
and [BP11-099 official page](https://shadowverse-evolve.com/cardlist/?cardno=BP11-099).
These are ability fragments with synthetic coordinates. BP01-041 proves lowering
and well-formedness only: D1 activation currently admits follower holders only.

`spell.yaml` transcribes the second line of **SD03-005 知恵の光** from
`card-data/jp/SD03/000-099.json`
([official page](https://shadowverse-evolve.com/cardlist/?cardno=SD03-005)).
Its printed cost is 1. Only the draw body is represented; this does not admit its
Quick keyword or an entire authored card. As above, source coordinates are synthetic.
The test supplies the printed cost explicitly to the candidate compiler; D1 checks
it against the generated snapshot before accepting the Spell header. The E2E
retains Printed origin and runs the ordinary payment/resolution route unchanged.

The play golden's target/distribution and ChoiceKeyword bodies are synthetic
replacements of that fragment, not transcriptions of 知恵の光. Target selectors
currently preserve only unfiltered public field sets; D1 additionally rejects a
position containing any non-follower target candidate. Kind filters are rejected,
not erased. Distribution uses one preceding target group, a constant total and a
minimum of one per recipient. ChoiceKeyword accepts a sole top-level choice with
constant exact count and independent modes, including 1-based `option` targets
visible only inside their mode. Nested choices, mode-local Distribution, mixing
mode and unconditional targets, and exclusion histories remain unsupported.
The extra mode E2E uses a synthetic body with one target-requiring mode and one
draw mode; selecting the latter does not ask for the former's unavailable target.
The activation golden additionally appends a synthetic `self_to_cemetery` cost
to assert its ordered owner-cemetery Put shape; it is not part of BP01-041's wording.
