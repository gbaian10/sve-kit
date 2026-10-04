# R2 preview upload

This command validates the complete public preview transport and uses the R2
S3-compatible API for conditional object writes. It does not build a snapshot,
read source archives or open the crawl manifest. Brotli uses the locked PyPI
`brotli` package. Signing and S3 request/response serialization use the locked
`boto3` SDK; strict S3 types come from the maintained `types-boto3[s3]` dev
dependency (the successor to `boto3-stubs`).

`sdk.py` isolates SDK configuration and package-model loading from AWS
environment variables, profiles and user model directories. Its SDK event
boundary permits one send per operation, preserves undecoded object bytes and
bounds XML input before the SDK parses it. Debug logging for SDK/HTTP namespaces
is suppressed while the client is open and restored on close, so credentials
and signed headers cannot enter debug reports. The CLI uses clients sequentially;
share neither a client nor its logging scope between concurrent publishers.

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

Every retained manifest must be a complete JP or sorted EN+JP preview under the
existing publication contract. EN-only and other regional scopes remain rejected.
Validation checks content addresses, sizes, independent
snapshot and full-text readers, reference closure, canonical gzip bytes, decoded
Brotli content,
public image bindings, all five image sizes, and decoded WebP format/dimensions.
Unknown files, PNG, private fields/recipes, local paths, symlinks, special files,
and unreferenced objects fail before HTTP. Original root-relative `img src`
values remain web references when their source URL has a public HTTPS base;
filesystem path forms are rejected in those fields too.

Optional `.br` siblings are validated by bounded decompression against their raw
JSON, regardless of producer version or quality. The checked-in lockfile supplies
the Python Brotli package; no system library, shell wrapper or executable pin is
required. Invalid, truncated, trailing or mismatched streams fail offline.
Existing preview files are read without alteration or recompression.

For a new snapshot, `snapshot export` and `snapshot export-offline` accept
`--brotli` (default `--no-brotli`). Encoding uses generic mode, quality 11 and
lgwin 22, and records the package version in the private compression recipe.
The public raw JSON and gzip rules are unchanged. Upload commands need no
Brotli option. Frozen formal bundles retain their exact manifest and changes
representations during dry-run and retry; decoding does not authorize replacing
an existing immutable object with a different encoding.

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

The endpoint is fixed to HTTPS on the
explicit account's `r2.cloudflarestorage.com`; proxies, environment HTTP options
and redirects are disabled. This uses signed GET and PUT requests, not the
public CDN URL. Unset the credential variables after use.

Images, blobs and manifests are immutable version members. Missing members use
`If-None-Match: *`; existing members are checked against exact bytes and content
metadata and skipped. A differing existing object stops the upload. A concurrent
conditional-create winner is re-read and must match. Normal member publication never overwrites, deletes or repairs an existing
object. Each execute also probes conditional-write enforcement as described below. Compressed siblings retain their own keys and
`application/octet-stream`, without `Content-Encoding`; raw JSON uses
`application/json` and WebP uses `image/webp`. Development immutable responses
use `private, max-age=31536000, immutable`; the pointer uses `no-store`.

Images go first, then blobs, then manifests. Only after all members are verified
and the local inventory is revalidated does the pointer change, using its opaque
ETag with `If-Match` or `If-None-Match: *` for first publication. A pointer race
stops instead of overwriting another publisher. Freeze the local preview during
execution; do not edit it or run two publishers concurrently.

After verifying the first immutable member, execute sends its identical bytes
with `If-None-Match: *`, then an intentionally wrong `If-Match` ETag. Both must
return 412; success stops before any further members or the pointer. This adds no
probe key or delete. A platform ignoring conditions could rewrite the first
member's identical bytes before detection, so the probe cannot prove zero writes
on a broken platform. It does not replace a maintainer's real concurrency test.

GET retries only transient timeouts, network errors and remote protocol errors:
three attempts total, with waits of 0.5 and 1 second. HTTP status, validation and
local protocol errors are not retried. PUT is sent once; losing its response stops
execution so rerunning can verify state without altering conditional semantics.
The client has a 30-second per-I/O timeout and no whole-run deadline.

## Execution and rerun cost

The measured preview has 33,863 files / 1,121,058,055 bytes. One local dry-run,
using the former 203-member Brotli recompression check, took about five minutes.
That historical timing does not measure the current decompression check. Execute
performs three complete local validations before accounting for network work.
Tests use tiny shared synthetic bases rather than this production comparison.

For 33,862 immutable members and one pointer, first publication requires about
101,591 sequential requests without retries and about 1.12 GB upload plus 1.12 GB
verification download. A fully matching rerun requires about 33,866 requests,
including two expected-412 probe PUTs, and still downloads about 1.12 GB. Counts
include existing-pointer inspection and final verification. Interruption reruns
verify earlier members again; remote matching is never inferred from an old log.
Actual R2 latency, billing and Access behavior are unverified and must be measured
by the maintainer. Avoid repeated full reruns before diagnosing a failure.

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
requires authentication. Workers static-assets configuration, frontend remote-preview wiring and Cloudflare setup
are separate work; this uploader alone does not put the environment online.
