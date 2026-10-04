# R2 publication of snapshot 2.0

`r2 upload-v2` connects the injected 2.0 publisher to signed S3 requests and ordinary
CDN GETs. The existing `r2 upload-preview` remains the unchanged 1.x preview path;
this command accepts only an already formally gated, frozen **2.0** release.
It never changes `preview-*` into a formal data version. The legacy 1.x uploader
does not join the new writer lease; do not run it concurrently against the same
bucket as a 2.0 publisher/collector. No credentials or account,
bucket or CDN hostname are stored in the repository or passed through CI.

## Offline preparation

The caller remains responsible for the source/adoption/publication gates in #34.
Initialize the publisher's `Ledger(primary_root, backup_root)` explicitly only for
a first deployment. Reserve a unique formal `data_version` before producing media,
use the returned revision with `prepare_media`, export and formally gate the
snapshot upstream, then call `snapshot.publish.plan.prepare`. Automatic promotion,
initialization, recovery, supersession and allocation are intentionally absent
from the upload CLI. Failed and unused reservations must remain in the ledger.

The helper `r2_upload.v2.bundle.write_bundle(empty_absolute_directory, release)`
freezes a validated `Release` into:

- `release.json`: private format-1 descriptor, manifest path, reserved media state,
  verified assets/source hashes, image confirmation IDs and compression recipe;
- `snapshots/blobs/` and `snapshots/manifests/`: exact immutable raw/gzip/optional
  Brotli members, including adjacent changes when present;
- `sources/`: verified, private content-addressed WebP inputs. Their upload keys
  come from the permanent printing/face paths in the media plan.

`release.json` and `sources/` are never uploaded. Keep this bundle outside git;
it can contain official text. The loader validates closure, canonical hashes,
encodings, image bytes/dimensions, reservation/committed basis and exact inventory;
it refuses unexpected files, symlinks and preview manifests before credentials
or HTTP clients. It does not open an archive, crawl manifest or latest cache.
The bundle is a handoff boundary for a formally gated build, not a new build or
an invented approval receipt.

Save the ledger checkpoint **independently** with
`r2_upload.v2.bundle.save_checkpoint(checkpoint_file, ledger)` after reservation
and preparation. Use a mode-600 file with an existing absolute parent outside
both ledger roots and an independently backed-up location. The uploader requires
an exact current pin; it never replaces a missing/stale pin before execution.
An invalid ledger/backup stops the command. Crash recovery still uses the verified
publisher recovery API and independent evidence, not current/previous alone.

```bash
uv --directory carddb run sve-carddb r2 upload-v2 \
  --release-dir /explicit/frozen-release \
  --ledger-dir /explicit/primary \
  --backup-dir /independent/backup \
  --checkpoint-file /third/checkpoints/release.json \
  --cdn-base-url https://cdn.example.invalid/ --dry-run
```

Dry-run is the default. It neither reads credentials, creates an HTTP client nor
changes state/checkpoints. Counts and bytes are complete local **candidates**, not
remote missing-object counts. Image `v` ranges distinguish all display tokens from
the newly reserved token. Index size/previous come from durable receipts; execute
must reconcile them with the real index. No remote inventory can be discovered
without I/O. `would_collect` is empty because collection requires a separate
per-run inventory and consent, rather than a guessed list of obsolete objects.
A Brotli bundle requires the same pinned
`--brotli-command` as its producer; gzip-only needs no external compressor.

## Explicit maintainer execution

Add `--execute --confirm-maintainer-authorization` only after contemporary
maintainer approval and separate development acceptance. Target selection uses
explicit `--account-id`/`--bucket`, or `R2_ACCOUNT_ID`/`R2_DEV_BUCKET` only in execute
mode. Credentials are read solely from `SVE_R2_ACCESS_KEY_ID` and
`SVE_R2_SECRET_ACCESS_KEY`; missing values fail without HTTP. No env file, profile,
metadata service or ambient AWS credentials are read. Production and development
must have different operator-scoped credentials. Agents do not execute this
against a real service. CI only runs the synthetic tests; no R2 secrets belong
in Actions.

The HTTPS endpoint is derived from the account and bucket. SigV4 signs the exact
conditional headers, payload and sorted percent-encoded query parameters, including
list continuation tokens. PUT uses `If-None-Match: *` or an opaque `If-Match` ETag;
412 means a failed condition. Unsupported conditions/statuses, redirects and
transport failures stop, with no plain-PUT fallback or write retry. GET preserves
wire bytes of compressed siblings rather than transparently decoding them. Both
clients disable proxies/ambient configuration and redirects; per-I/O timeout is
30 seconds, with no whole-run deadline. Responses are bounded to 128 MiB per object
and 2 MiB per inventory page; exceeding the bound fails, not truncates. Errors omit
response bodies, request headers, credentials and signatures. Do not enable HTTP
wire logging or `--showlocals` around execution.

All cooperating publishers/collectors, including on other machines, share
`coordination/snapshot-v2-writer.json`. It is a non-expiring CAS lease with a unique
owner and `no-store` metadata. Unlock is another conditional PUT to the idle record;
it never uses DELETE. Crash or uncertain acquisition leaves the active record and
stops other writers; there is **no timed takeover**. Operator recovery must prove
all writers stopped and reconcile origin/index/ledger before conditionally
releasing a stuck lease. Other writers that ignore the lease remain forbidden.
The uploader never automates that recovery or edits the existing local wrapper.

Origin images and immutable JSON are verified before the current/previous index
CAS. CDN verification requests the exact full image URL including `?v=`, without
authorization headers, redirects, cache bypass, cache busting or purge. HTTP denial
(including Access), encoded or poisoned/negative-cache bytes block publication.
Normal publication performs four CDN GETs per current image URL; an already
committed retry performs two. This can be expensive and needs live acceptance.

After successful or failed durable publication, the independent checkpoint is
advanced and fsynced; failed attempts keep their reservation/sealed plan. If an
uncertain process death occurs before this pin update, subsequent execution stops
on the stale pin. Reconcile independent evidence with the operator instead of
silently generating a replacement. Retry an unchanged bundle/version only after
review; changed input or unknown external image bytes are refused. A failed write
may have changed origin bytes without switching current, as defined by #297.

## Per-run collection and acceptance boundary

Cloudflare's [S3 compatibility table](https://developers.cloudflare.com/r2/api/s3/api/)
explicitly lists conditional PUT but does **not** promise atomic `If-Match` on
`DeleteObject`. The generic conditional `R2Store.delete` still fails before HTTP;
it never silently downgrades that contract. The separate `r2 gc-v2` workflow uses
the maintainer-approved single cooperating writer model and **unconditional
DELETE**, rather than claiming atomic conditional deletion. Every writer must
honor the same lease, including maintenance scripts. A writer bypassing the lease
can race between the final reads and DELETE; this workflow cannot protect against
such non-cooperating writes.

The default command only displays an already saved local approval list. It reads
no credentials and performs no HTTP. Explicit `--inspect-remote` requires
`--confirm-maintainer-authorization` and one or more exact `--namespace` flags.
It checks the independently pinned ledger, acquires the writer lease, reconciles
the remote current/previous index with durable receipts, and records unreferenced
keys, ETags, hashes, counts and bytes in a new mode-600 `--plan-file`. Public objects
are only read; acquiring/releasing the coordination lease uses conditional PUT.
Only `snapshots/blobs/`, `snapshots/manifests/` and each explicit public image-size
prefix are accepted. An irregular key aborts the entire inspection, rather than
being skipped. Raw, inventory, crawl manifests, backups and authored data are
outside these namespaces.

If `snapshots/preview/current.json` exists, **the entire GC is refused**, including
for a valid preview pointer. Legacy previews share these manifest/blob namespaces
but do not use this writer lease. This collector does not try to interpret or
discard their closure; malformed, unknown-format and legacy 1.x pointers also
block deletion. Inspection and execution replan check the pointer under their
lease, with another check before deletion. Use a separate preview bucket if GC
must coexist with a live preview; this command never removes the pointer.

Both retained versions' JSON closures remain protected, but public images retain
only the **current** image set (ADR-0016). Previous-only image paths are collectible;
they are not required to exist when verifying the retained closure. Legal
sealed/failed attempts' staged members and assets remain protected separately.
A missing retained member stops collection. The
collector must use the publisher's same complete ledger and backup; another
machine's incomplete copy cannot prove the absence of unfinished releases.
This matches the generic #297 `collect` retention rule. Its conditional deletion
contract remains unsupported by R2Store; use the separately approved `gc-v2`
workflow for R2 deployment collection.
The inspection releases the lease while the maintainer reviews its saved list; it
does not hold a lock across an unbounded human wait. Actual execution requires
`--execute --confirm-maintainer-authorization --confirm-delete 'DELETE-V2 sha256:…'`,
where the exact confirmation string is printed with that list. Execution acquires
the lease and reproduces the full approved inventory under it. Any inventory,
candidate, receipt or index change requires a new list and fresh consent.
**The execution replan and every deletion share that single writer lease.**
Immediately before each unconditional DELETE it rechecks candidate bytes/ETag,
the full current/previous index and ownership of the lease. Lost ownership stops
without deleting; a DELETE transport failure is never retried automatically.
Partial completion requires another inspection and consent, not replaying a
stale list. Collection is never automatic after publication.

```bash
uv --directory carddb run sve-carddb r2 gc-v2 \
  --plan-file /private/reviews/gc.json \
  --ledger-dir /explicit/primary --backup-dir /independent/backup \
  --checkpoint-file /third/checkpoints/release.json
```

This example is an offline display of an existing list; add the explicit remote
flags only after authorization. Target/credential inputs follow `upload-v2`.
ListObjectsV2 is prefix-restricted and checks bounded UTF-8 XML and pagination,
but listing alone does not enable deletion. No actual R2 conditional support has been
measured by these localhost tests.

Real R2 consistency/conditions, deployed Cache Rules, full-query cache separation,
Access, browsers and SW still require separate authorized verification before
use. Purge is a separate permission and absent here. Checkpoint/ledger backup,
formal gates and a working public CDN path remain operator responsibilities.
