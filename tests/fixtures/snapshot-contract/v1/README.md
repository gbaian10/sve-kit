# Synthetic snapshot contract fixtures

These are handwritten synthetic examples, not official card text or producer
output. Wire tuples and the expected logical objects were authored separately.
`index.json` lists the shared inputs for Python and TypeScript readers.

The candidate profile is format `1.0.0`, bucket count `1`. Files are formatted for
review; manifest hashes and sizes refer to canonical-json-v1 bytes. Preserve
`raw_json` strings in negative cases before parsing to avoid losing numeric syntax.

See the [contract guide](../../../../docs/schema/snapshot-contract.md) for the
mutation procedure, validation boundaries, Python command and TS checklist.
Changing these fixtures requires reviewing both the wire representation and the
independently specified expected objects. Never refresh expected objects from a
reader or producer to make a test pass.

The golden includes five image variant metadata rows, printing-face sections and
corrections, numeric/variable/literal spellings, and matching multilingual ruling
hints. Image paths are synthetic references; no image binaries are distributed.
Shared negative cases cover calendar/time limits, ASCII numeric syntax, URI
encoding, undeclared or disabled parameters, and inconsistent hint declarations.
An optional `error` fragment checks the intended Python failure; other readers
must reject for the same reason without copying Python exception wording.
