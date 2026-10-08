# R2 upload

The `r2` CLI uploads an `export-offline` preview root as snapshot **2.0**
(`upload-v2`) and collects objects outside the remote current/previous window
(`gc-v2`). See [the upload guide](v2/README.md).

`commands.py` registers the two commands. `snapshot/read_api.py` selects and
verifies the public closure and supplies redacted errors and non-symlink reads.
`sdk.py` owns the typed boto3 transport, explicit credentials and bounded
responses. In `v2/`, `headers.py` supplies transport metadata, `publish.py`
uploads the verified export, `adapter.py` is the R2 object boundary,
`freshness.py` checks the CDN and `gc.py` collects remote objects.
