# R2 preview upload

This command validates the complete public preview transport and uses the R2
S3-compatible API for conditional object writes. It does not build a snapshot,
read source archives or open the crawl manifest. It adds no dependency.

Run from the repository root with an absolute, non-symlink preview directory:

```bash
uv --directory carddb run sve-carddb r2 upload-preview \
  --preview-dir /explicit/preview --dry-run
```

Dry-run is also the default when neither mode flag is given. It reads no
credentials and creates no HTTP client. JSON output includes candidate file and
byte totals by kind, the pointed manifest hash, and the excluded `private` and
`reports` roots. These are local candidates; remote existence is **not checked**.
Private/report trees are neither traversed nor read.

Only these public keys are permitted:

- `images/sha256/<first-two-hex>/<64hex>.webp`
- `snapshots/blobs/<64hex>.json` and their `.gz` / optional `.br` siblings
- `snapshots/manifests/<64hex>.json` and their `.gz` / optional `.br` siblings
- `snapshots/preview/current.json`

Every retained manifest must be a complete JP preview under the existing
publication contract. Validation checks content addresses, sizes, independent
snapshot and full-text readers, reference closure, canonical compressed bytes,
public image bindings, all five image sizes, and decoded WebP format/dimensions.
Unknown files, PNG, private fields/recipes, local paths, symlinks, special files,
and unreferenced objects fail before HTTP. Original root-relative `img src`
values remain web references when their source URL has a public HTTPS base;
filesystem path forms are rejected in those fields too.

A preview containing Brotli siblings requires the producer's explicitly selected
local encoder, accepting `--version` and `-q 11 -c` with stdin/stdout bytes:

```bash
uv --directory carddb run sve-carddb r2 upload-preview \
  --preview-dir /explicit/preview --brotli-command /explicit/trusted-encoder
```

The selected encoder's output must match every existing `.br` byte for byte.
A local encoder may run even in offline mode; it must be a trusted offline
program. No encoder is discovered or installed implicitly. Gzip-only previews
need no external encoder.

## Maintainer execution

Every real upload requires the maintainer's current approval and is run by the
maintainer. Agents and CI must not receive the credentials or run this command
against a real service. Keep development and production buckets and tokens
separate; this command only publishes previews, not formal release indices.

In the maintainer's local shell, set `SVE_R2_ACCESS_KEY_ID` and
`SVE_R2_SECRET_ACCESS_KEY` without entering literal secrets in shell history.
Use a token scoped to Object Read & Write on the single development bucket;
reads are needed to verify existing objects. No credential file or AWS profile
is read. Do not save either variable in git, build output, CI or shared logs.

After offline reconciliation and current authorization:

```bash
uv --directory carddb run sve-carddb r2 upload-preview \
  --preview-dir /explicit/preview \
  --account-id "<32-lowercase-hex-account-id>" --bucket "<development-bucket>" \
  --execute --confirm-maintainer-authorization
```

Add `--brotli-command` when required. The endpoint is fixed to HTTPS on the
explicit account's `r2.cloudflarestorage.com`; proxies, environment HTTP options
and redirects are disabled. This uses signed GET and PUT requests, not the
public CDN URL. Unset the credential variables after use.

Images, blobs and manifests are immutable version members. Missing members use
`If-None-Match: *`; existing members are checked against exact bytes and content
metadata and skipped. A differing existing object stops the upload. A concurrent
conditional-create winner is re-read and must match. No member is overwritten,
deleted or repaired. Compressed siblings retain their own keys and
`application/octet-stream`, without `Content-Encoding`; raw JSON uses
`application/json` and WebP uses `image/webp`. Development immutable responses
use `private, max-age=31536000, immutable`; the pointer uses `no-store`.

Images go first, then blobs, then manifests. Only after all members are verified
and the local inventory is revalidated does the pointer change, using its opaque
ETag with `If-Match` or `If-None-Match: *` for first publication. A pointer race
stops instead of overwriting another publisher. Freeze the local preview during
execution; do not edit it or run two publishers concurrently.

## Stop and rerun

On interruption or any validation, HTTP or conditional-write error, stop and
review the local counts and remote state with the maintainer. Completed
immutable objects may remain in the bucket. Rerun the same verified local
preview with current authorization; matching objects are skipped and the pointer
is published last. Do not delete objects, force overwrite, or use a directory
sync tool to bypass checks. If the pointer PUT committed but its response was
lost, the pointer still references the already verified complete version;
a rerun verifies and skips it. Output omits response bodies and credentials.

Local synthetic tests verify SigV4 and conditional writes through MockTransport.
They do not establish real R2 conditional-write support or Access protection.
Before real data, the maintainer must verify these with a synthetic preview and
the development bucket, and confirm every public hostname and deployment alias
requires authentication. Frontend remote-preview wiring and Cloudflare setup
are separate work; this uploader alone does not put the environment online.
