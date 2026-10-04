# R2 publication

The `r2` CLI supports snapshot **2.0** publication (`upload-v2`) and separately
approved collection (`gc-v2`). See [the publication guide](v2/README.md) for frozen
release preparation, required ledger/checkpoint inputs and explicit execution.
Snapshot 1.x previews remain local exports and have no R2 upload command.

`commands.py` registers the two commands. `boundary.py` provides redacted errors
and non-symlink local member reads. `sdk.py` owns the typed boto3 transport,
explicit credentials and bounded responses; `v2/` implements the publication
adapter, bundle reader, CDN verification and collection workflow.
