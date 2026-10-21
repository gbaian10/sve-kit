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
`AuthorExpander`, passes only the selected expanded trees to Rust, and never
stores generated JSON or sidecars. These tests do not admit cards for production.
