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
