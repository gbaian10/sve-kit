# sve-publish

Image production and text export have independent upload commands. Run them in
order: **images upload → snapshot upload → optional collection**. A text-only
update needs only snapshot upload. One maintainer runs these operations serially;
image builds, uploads and either collector must not overlap. No credentials,
account, bucket or CDN hostname are stored in the repository or passed through CI.
Preview entries remain development data; formal releases still need #34's gates.

## Image handoff and inventory

A successful `sve-carddb images build-index` prints `image_upload`, pointing to
`image-upload/<sha256>.json` in its public output root. This canonical description
contains `format: sve-image-upload-v1`, `image_index_sha256`, producer `revision`
and sorted `objects`. Each object has exactly `key`, `version`, `sha256`, `bytes`.
Hashes come from verified producer outputs, without rereading unchanged bitmaps.
Private source URLs, evidence, paths and media state never enter this description.
Only a successful build's description and unchanged image output root may be used.

Each bucket independently records successful uploads in `_publish/images.json`:

```json
{"format":"sve-publish-images-v1","objects":[{"bytes":30590,"key":"images/card_s/123.webp","sha256":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","version":7}]}
```

The wire format is canonical JSON, sorted by key with unique keys. Versions and
sizes are positive integers; only the five public WebP namespaces are accepted.
Unknown formats, extra fields, malformed JSON or invalid metadata stop execution.
The inventory contains only public image information: production CDN readers may
fetch it. The development Worker refuses `_publish/`; publication uses S3 directly.
The inventory is neither a source archive nor a replacement for version-state backups.

## Dry-run

```bash
uv --directory publish run sve-publish images upload \
  --image-upload '/explicit/image-output/image-upload/<sha256>.json' \
  --image-dir /explicit/image-output --dry-run
uv --directory publish run sve-publish snapshot upload \
  --export-dir /explicit/text-output --dry-run
```

Dry-run is the default. Both commands open no HTTP client, read no credentials,
write nothing and never open bitmaps. Image dry-run checks the description;
snapshot dry-run checks the pointer, manifest hash/schema, JSON hashes/lengths,
Brotli decoding, reader join and reference closure, then reports image requirements.
Remote differences and readiness are unknown offline. `--export-dir` takes
precedence over `SVE_EXPORT_DIR`; a non-empty absolute root is required.

## Execute

```bash
uv --directory publish run sve-publish images upload \
  --image-upload '/explicit/image-output/image-upload/<sha256>.json' \
  --image-dir /explicit/image-output \
  --account-id "$R2_ACCOUNT_ID" --bucket "$R2_DEV_BUCKET" --jobs 16 --execute
uv --directory publish run sve-publish snapshot upload \
  --export-dir /explicit/text-output \
  --account-id "$R2_ACCOUNT_ID" --bucket "$R2_DEV_BUCKET" --jobs 16 --execute
```

Agents and CI never execute against a real service. The maintainer selects the
account/bucket explicitly or through `R2_ACCOUNT_ID`/`R2_DEV_BUCKET`. Only
`SVE_R2_ACCESS_KEY_ID` and `SVE_R2_SECRET_ACCESS_KEY` supply credentials; no secret
file, AWS profile, metadata service or ambient credentials are read.

Image upload reads the inventory once and compares descriptions. Identical rows
need no PUT or bitmap read. A lower version or different bytes under the same
version is refused. A new version with identical bytes changes only the inventory.
Before overwriting, a conditional inventory write removes all affected existing
rows, including version-only changes. Only new/changed candidates are read locally,
checked against SHA-256 and length, then PUT with contractual headers. There are
no per-image GETs, HEADs or read-backs. Only after all candidates succeed are new
rows conditionally committed, preserving unrelated remote rows.

Missing inventory (404) means a first baseline: upload all described images even
if the bucket already contains them. Other failures never mean an empty inventory.
Control writes use ETag conditions and read-back; an uncertain PUT response is
resolved by reading that control file only. A conflict stops execution. Images
use explicit ordinary overwrites under the single-writer rule; JSON stays create-only.
The inventory format also represents the image-header contract: future header
changes require an explicit retransmission/migration, even when hashes match.

Snapshot upload requires every referenced image's exact version and byte length
in the inventory. Missing/older rows require images upload; newer rows require a
text rebuild with the current image index. No bitmap directory is needed. JSON
objects retain GET comparison, conditional create-only PUT and read-back; immutable
collisions stop. The snapshot command rechecks the inventory, then conditionally
commits `snapshots/versions/index.json` last and reads it back. First revision is 1;
later versions retain the old current as previous. Reused versions with another
manifest and chronologically older exports are refused.

Neither command performs daily full-CDN verification. A successful image upload
claims origin PUT completion only (`cdn_verification: not_checked`). The maintainer
checks changed versioned URLs and repeats synthetic query/cache acceptance when
images or cache configuration change; see the [image contract](../docs/schema/images/image-variants.md).

### Failure and recovery

After a failure, stop and rerun **the same image description with the unchanged
image root** before switching producer batches or publishing snapshots. Missing
claims block snapshots. Already uploaded candidates may be retransmitted; no
per-file checkpoint is maintained. Unchanged successful reruns make zero PUTs.
Do not restore older batches after an interrupted overwrite: removed rows no
longer retain a complete version high-water history. Back up producer media state.

`--jobs` caps both object worker pools (default 16, minimum 1). A failure stops new
PUT admission and drains in-flight operations; admitted writes may still finish.
Progress goes to stderr every 100 completions or about five seconds; stdout is a
JSON summary. Each I/O has a 30-second timeout, no write retry or plain-PUT fallback
for conditional objects. Responses are bounded to 128 MiB per object and 2 MiB
per list page. Errors omit response bodies and credentials; avoid wire logging.

| Object | Content-Type | Cache-Control | Content-Encoding |
| --- | --- | --- | --- |
| JSON | `application/json` | `public,max-age=31536000,immutable` | none or `br` |
| WebP | `image/webp` | `public,max-age=86400,must-revalidate` | none |
| version index / image inventory | `application/json` | `no-store` | none |

## Collection

```bash
uv --directory publish run sve-publish snapshot gc \
  --account-id "$R2_ACCOUNT_ID" --bucket "$R2_DEV_BUCKET" --dry-run
uv --directory publish run sve-publish images gc \
  --account-id "$R2_ACCOUNT_ID" --bucket "$R2_DEV_BUCKET" --dry-run
```

GC dry-run is remote read-only and needs credentials; `--execute` deletes.
Snapshot GC retains current/previous JSON closures and optional manifest `.br`,
without reading images, image inventory or current payload joins. Image GC reads
current's verified payloads to derive image references, retains current images
only, and uses paginated LIST to find unretained images including upload orphans.
It refuses missing retained objects/claims, conditionally removes candidate claims
**before DELETE**, and checks the version index before deletion. An interrupted
DELETE leaves unadvertised orphans; rerun LIST to finish. Neither collector deletes
`_publish/images.json`, source history or local evidence. `changes.from` never
retains a third snapshot. `--namespace` accepts only that command's exact prefixes.

GC never runs automatically. Do not run it between images upload and snapshot
upload: current does not yet retain the next images. If collected then, their
missing inventory claims block publication and images upload restores them.
Tool-external edits/deletions can invalidate claims and are outside this trust
model. To repair, delete `_publish/images.json` while no writers run; the next
images upload then treats the bucket as a first baseline and reuploads every
described image. ETag conditions detect conflicts, not a distributed lock.

## Maintainer development rehearsal

Before deployment acceptance, record real inventory size and timings; confirm
first baseline then zero image PUT/GET on rerun, a text-only upload without image
mounts, and controlled small synthetic overwrite/GC interruptions with same-input
recovery. Verify R2 conditional control writes, dev `_publish/` denial after login,
Access protection before login, and changed `?v=` URLs/browser/SW cache behavior.
Offline fake-SDK tests do not establish platform configuration or deployment success.

## Development

```bash
uv --directory publish sync
uv --directory publish run ruff check
uv --directory publish run ruff format --check
uv --directory publish run mypy
uv --directory publish run pytest -n 4 --cov
```

Production imports only `sve_carddb.export.read_api` from the editable `../carddb`
dependency. Synthetic fixtures use real producers and fake SDK transports. Tests
block external networking and writes outside pytest temporary roots, except
coverage/bytecode artifacts. Combined line/branch coverage must reach 92%.
