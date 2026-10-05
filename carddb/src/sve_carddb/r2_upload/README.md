# R2 upload

The `r2` CLI uploads an `export-offline` preview root as snapshot **2.0**
(`upload-v2`) and collects objects outside the remote current/previous window
(`gc-v2`). See [the upload guide](v2/README.md).

`commands.py` registers the two commands. `boundary.py` provides redacted errors
and non-symlink local member reads. `sdk.py` owns the typed boto3 transport,
explicit credentials and bounded responses. In `v2/`, `export.py` selects and
verifies the export's public closure, `publish.py` uploads it, `adapter.py` is
the R2 object boundary, `freshness.py` checks the CDN and `gc.py` collects.
