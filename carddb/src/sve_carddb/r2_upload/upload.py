"""Create-only members first, an optimistic atomic preview pointer last."""

from typing import TYPE_CHECKING

from sve_carddb.r2_upload.plan import (
    POINTER,
    Member,
    Plan,
    UploadError,
    plan_preview,
    pointer_value,
    read_member,
)
from sve_carddb.snapshot.values import digest

if TYPE_CHECKING:
    from sve_carddb.r2_upload.s3 import S3, Remote
    from sve_carddb.snapshot.export import Brotli


def _same(remote: Remote | None, member: Member) -> None:
    if (
        remote is None
        or len(remote.raw) != member.size
        or digest(remote.raw) != member.sha256
    ):
        raise UploadError("Existing R2 object differs from the local public member")
    if (
        remote.headers.get("content-type") != member.content_type
        or remote.headers.get("cache-control") != member.cache_control
    ):
        raise UploadError(
            "Existing R2 object metadata differs from the upload contract"
        )


def _probe(remote: S3, member: Member, raw: bytes, existing: Remote) -> None:
    etag = existing.headers.get("etag")
    if not etag:
        raise UploadError("Conditional-write probe requires an object ETag")
    # Identical bytes on a verified member avoid introducing a separate public probe object.
    for condition in (
        {"if-none-match": "*"},
        {"if-match": '"' + etag.strip('"') + '-probe"'},
    ):
        if remote.put(
            member.key,
            raw,
            condition
            | {
                "content-type": member.content_type,
                "cache-control": member.cache_control,
                "x-amz-meta-sha256": member.sha256,
            },
        ):
            raise UploadError("R2 did not enforce conditional writes")


def _immutable(plan: Plan, remote: S3, member: Member, *, probe: bool = False) -> bool:
    raw = read_member(plan.root, member.key)
    if len(raw) != member.size or digest(raw) != member.sha256:
        raise UploadError("Local public member changed")
    existing = remote.get(member.key, limit=member.size)
    created = False
    if existing is None:
        created = remote.put(
            member.key,
            raw,
            {
                "if-none-match": "*",
                "content-type": member.content_type,
                "cache-control": member.cache_control,
                "x-amz-meta-sha256": member.sha256,
            },
        )
        existing = remote.get(member.key, limit=member.size)
    _same(existing, member)
    if probe:
        assert existing is not None
        _probe(remote, member, raw, existing)
    return created


def _pointer(plan: Plan, remote: S3, prior: Remote | None, pointer: Member) -> bool:
    raw = read_member(plan.root, POINTER)
    if digest(raw) != pointer.sha256:
        raise UploadError("Local public member changed")
    if prior is not None and prior.raw == raw:
        _same(remote.get(POINTER, limit=pointer.size), pointer)
        return False
    condition = (
        {"if-none-match": "*"} if prior is None else {"if-match": prior.headers["etag"]}
    )
    if not remote.put(
        POINTER,
        raw,
        condition
        | {
            "content-type": pointer.content_type,
            "cache-control": pointer.cache_control,
        },
    ):
        raise UploadError("Preview pointer changed concurrently; rerun after review")
    _same(remote.get(POINTER, limit=pointer.size), pointer)
    return True


def upload(
    plan: Plan, remote: S3, *, brotli: Brotli | None = None
) -> dict[str, object]:
    """Revalidate before I/O and publication; never delete or overwrite members."""
    if plan_preview(plan.root, brotli=brotli) != plan:
        raise UploadError("Local public inventory changed")
    prior = remote.get(POINTER, limit=4096)
    if prior is not None:
        pointer_value(prior.raw)
        if not prior.headers.get("etag"):
            raise UploadError("Existing preview pointer has no ETag")
    counts = {
        "uploaded_files": 0,
        "uploaded_bytes": 0,
        "skipped_files": 0,
        "skipped_bytes": 0,
    }
    first = True
    for member in plan.members:
        if member.key != POINTER:
            category = (
                "uploaded"
                if _immutable(plan, remote, member, probe=first)
                else "skipped"
            )
            first = False
            counts[category + "_files"] += 1
            counts[category + "_bytes"] += member.size
    if plan_preview(plan.root, brotli=brotli) != plan:
        raise UploadError("Local public inventory changed")
    pointer = next(m for m in plan.members if m.key == POINTER)
    category = "uploaded" if _pointer(plan, remote, prior, pointer) else "skipped"
    counts[category + "_files"] += 1
    counts[category + "_bytes"] += pointer.size
    return plan.report() | {"mode": "execute", "remote_existence": "verified"} | counts
