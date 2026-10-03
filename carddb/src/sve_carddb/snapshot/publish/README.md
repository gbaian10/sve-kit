# Snapshot 2.0 publication boundary

This package implements offline-tested orchestration. It contains no network
client, credential loading, upload CLI, CDN purge, or deployment authorization.
The formal approval gates tracked by #34 still belong to the caller; a successful
synthetic test does not authorize publication. Preview manifests are rejected.

## Preparing and publishing

1. Construct `Ledger(primary_root, backup_root)` with two disjoint, durable roots
   outside the repository and public object namespaces. `initialize()` is only
   for the first deployment and refuses existing state or recovery evidence.
2. Call `reserve(data_version)` before generation. The reservation journal is
   appended and fsynced before primary state changes. State and its backup are
   atomically written, fsynced, and verified before the revision is returned.
   Failed and unpublished reservations consume numbers; never reuse a version.
3. Use that revision and `ledger.media_basis()` with `prepare_media`. Then supply
   the resulting `MediaPlan`, the complete formally gated `Snapshot`, verified
   source blobs, an explicit HTTPS CDN root, and adjacent changes to
   `plan.prepare`. The caller must supply the pinned compressor when Brotli
   representations exist and explicit per-image approval evidence when required
   by the media producer. This API never promotes preview output to formal data.
4. Inject an `ObjectStore` and `Freshness` implementation into `publish`. The
   store must provide an exclusive deployment-wide publisher/collector lease,
   opaque ETags, create-only writes, and conditional overwrite/delete. A real
   adapter must coordinate processes and machines; a process-local mutex is
   insufficient. These requirements apply to every cooperating writer.

Before output, the publisher revalidates the complete transport against the
independent reader and recomputes image groups against durable committed state.
It pins output hashes, URLs, tokens, prior index and per-image ETags in backed
state. Only changed image bytes are PUT. All current image URLs, including an
unchanged size in a changed group, receive two ordinary cache-preserving CDN GETs
and full SHA/length checks. Opaque, missing, poisoned and negative-cache responses
block publication. JSON is create-only, with exact raw and compressed bytes and
headers checked. The no-store current/previous index is switched last using CAS.

A failed overwrite can leave new origin bytes while current remains unchanged;
fixed image keys are not historical storage. Resume the same pinned plan with
the same data version and tokens. Changing its outputs is refused. An explicit
`supersede(revision)` permits a different reserved release, forces fresh image
tokens, and requires rechecking all output bytes. It does not silently declare
staging successful. An uncertain index commit is reconciled on retry using its
exact prepared index; its durable receipt is then finalized without new numbers.
Never roll a committed release back over newer current.

## Backup and recovery

Preserve the primary ledger, its separate backup and the complete append-only
reservation journal, including failed attempts. `checkpoint()` returns the
high-water mark and full hashes of verified state and receipts. The backup
workflow must retain that checkpoint independently, update the pin after every
reservation/state change, and verify restored copies against it. Keeping the pin
only beside the same potentially rolled-back copy cannot prove the latest bound.
No external publication proceeds when the primary and backup disagree.

When primary state is lost, `recover(observed_max=..., proof=checkpoint)` requires
the previously pinned checkpoint, matching backup and complete reservation chain.
The maximum independently observed image token is an additional lower bound,
not sufficient recovery evidence. Missing, stale or truncated evidence stops the
allocator. Recovery does not infer an upper bound from two retained snapshots,
a numeric gap, a timestamp, or current alone. A crash after reservation fsync but
before state persistence also stops automatic allocation; verified evidence
repair belongs to the operator, not a guessed restart at zero.

Durable publication receipts preserve manifest/index hashes and first-publication
identity event mappings. The full text-digest registry preserves `(language,
short text ID)` collision detection after public GC. Historic full snapshots and
official text are not retained in the ledger.

## Restricted collection

`collect` requires the committed revision and explicitly selected prefixes from:

- `snapshots/blobs/` and `snapshots/manifests/`
- `images/card_s/`, `images/card_m/`, `images/card_l/`
- `images/art_s/` and `images/art_m/`

It retains the JSON closure and listed compressed representations of current and
previous, shared content-addressed members, and legal in-flight outputs. A
`changes.from` identifier never recursively retains historic releases. WebP
retention uses current only; previous metadata does not retain old image bytes.
Unrecognized keys and all undeclared namespaces are left untouched. Missing
retained members fail closed before deletion. The shared writer lease, repeated
index ETag/content checks and conditional object deletion guard each collection.
Raw sources, inventory, source manifests/backups and authored data are outside
this allowlist. The ledger and its backup must also remain outside it.

Real R2 behavior, deployed Cache Rules/headers, query-key separation, browser/SW
integration and formal source/adoption gates still need separate authorized
verification before any live adapter can be used. Purge requires its own explicit
permission; these interfaces neither perform it nor use it to bypass freshness.
