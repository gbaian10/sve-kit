# Private page tests

The public case index contains file pins and field hashes, without official text.
Its validation test runs even when the private checkout is unavailable.

`SVE_PRIVATE_TESTDATA_MODE` defaults to `excluded`. Set it to `required` and set
`SVE_PRIVATE_TESTDATA_DIR` to a private testdata checkout to run the real-page cases.
Local tests verify file hashes; CI also verifies the pinned Git commit.

Do not add `--showlocals` or `-l` when running these tests, locally or in CI.
The fixture wrapper has a safe representation for pytest's default traceback,
but intermediate parser values in local variables still contain private text.
Failure locations use case IDs and field paths. Keep raw reports private and
never upload them to an artifact or cache.
