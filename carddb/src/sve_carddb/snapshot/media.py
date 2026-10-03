"""Verified 2.0 media projection and printing-face image revision comparison.

The publisher supplies a durably reserved revision and the last committed state.
This module does not allocate production revisions or publish remote assets.
"""

from dataclasses import dataclass, replace
from pathlib import Path, PurePosixPath
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.snapshot.preview.images import image_blobs
from sve_carddb.snapshot.values import (
    array,
    canonical,
    digest,
    integer,
    object_value,
    string,
)

if TYPE_CHECKING:
    from collections.abc import Iterator

    from sve_carddb.snapshot.project import Projection
    from sve_carddb.snapshot.project.source import Record

MAX_SAFE = 9007199254740991
SIZE_KEYS = ("art_m", "art_s", "card_l", "card_m", "card_s")


def _uint(value: int, *, positive: bool) -> int:
    if type(value) is not int or not (1 if positive else 0) <= value <= MAX_SAFE:
        raise ValueError("Image URL integer outside safe domain")
    return value


def image_path(int_id: int, ordinal: int, size: str) -> str:
    """Use permanent identities, never a card number or face array position."""
    _uint(int_id, positive=True)
    _uint(ordinal, positive=False)
    if size not in SIZE_KEYS:
        raise ValueError("Unknown image size")
    suffix = "" if ordinal == 0 else "-f" + str(ordinal)
    return "images/" + size + "/" + str(int_id) + suffix + ".webp"


def image_url(int_id: int, ordinal: int, size: str, version: int) -> str:
    """The query version belongs to the card or art group, not the file key."""
    _uint(version, positive=True)
    return image_path(int_id, ordinal, size) + "?v=" + str(version)


@dataclass(frozen=True)
class MediaPlan:
    projection: Projection
    state: dict[str, JsonValue]
    assets: tuple[dict[str, JsonValue], ...]

    def blobs(self, source: Path) -> Iterator[tuple[str, bytes]]:
        """Recheck full bytes so a changed build cache cannot corrupt a sealed plan."""
        from sve_carddb.store import resolve_within  # ruff: ignore[import-outside-top-level] -- keep the read-only filesystem boundary local

        for asset in self.assets:
            raw = resolve_within(
                source, PurePosixPath(string(asset["source"]))
            ).read_bytes()
            if digest(raw) != asset["sha256"] or len(raw) != integer(asset["bytes"]):
                raise ValueError("Media plan source hash or bytes mismatch")
            yield string(asset["path"]), raw


def _tokens(
    sizes: list[Record],
    binding: JsonValue,
    active: bool,
    before: Record,
    revision: int,
    committed: int,
) -> tuple[Record, Record]:
    fingerprints: dict[str, JsonValue] = {}
    versions: dict[str, JsonValue] = {}
    for group in ("card", "art"):
        fingerprint = (
            digest(
                canonical(
                    [
                        {
                            f: r[f]
                            for f in (
                                "size_key",
                                "width",
                                "height",
                                "bytes",
                                "path",
                            )
                        }
                        for r in sizes
                        if string(r["size_key"]).startswith(group + "_")
                    ]
                )
            )
            if active
            else None
        )
        fingerprints[group] = fingerprint
        prior_version = before.get(group + "_version")
        unchanged = (
            active
            and before.get("active") is True
            and before.get("binding") == binding
            and before.get(group) == fingerprint
        )
        if unchanged:
            _uint(integer(prior_version), positive=True)
            if integer(prior_version) > committed:
                raise ValueError("Media token exceeds committed revision")
        versions[group + "_version"] = (
            prior_version if unchanged else revision if active else None
        )
    return fingerprints, versions


def prepare_media(  # ruff: ignore[too-many-locals] -- indexes, validated source rows and output descriptors are kept separate at this boundary
    projection: Projection,
    source: Path | None,
    *,
    revision: int,
    previous: JsonValue = None,
    confirmed_images: frozenset[str] = frozenset(),
) -> MediaPlan:
    """Verify private content-addressed outputs, then compare each card/art group.

    Failed reservations are the caller's responsibility and must never be reused.
    A previous state must come from the last committed release, not from a failed
    candidate. Tombstones distinguish restoration from a never-seen binding.
    """
    _uint(revision, positive=True)
    old = {} if previous is None else object_value(previous)
    if old and (
        set(old) != {"revision", "members"} or revision <= integer(old["revision"])
    ):
        raise ValueError("Media revision must advance the committed state")
    members = {} if not old else object_value(old["members"])
    # Decode each unique source once before accepting its filename digest as evidence.
    for _path, _raw in image_blobs(projection.tables, source, confirmed_images):
        pass
    images = {string(r["id"]): r for r in projection.tables["image_asset"]}
    prints = {string(r["id"]): r for r in projection.tables["printing"]}
    faces = {string(r["id"]): r for r in projection.tables["face"]}
    variants: dict[str, list[Record]] = {}
    for row in projection.tables["image_variant"]:
        variants.setdefault(string(row["image_id"]), []).append(row)
    result: list[Record] = []
    assets: list[dict[str, JsonValue]] = []
    next_members: dict[str, JsonValue] = {
        key: {
            "active": False,
            "binding": None,
            "card": None,
            "art": None,
            "card_version": None,
            "art_version": None,
        }
        for key in members
    }
    seen: set[str] = set()
    paths: set[str] = set()
    for row in projection.tables["printing_image"]:
        printing, face, image = (
            string(row[f]) for f in ("printing_id", "face_id", "image_id")
        )
        key = canonical([printing, face]).decode()
        if key in seen:
            raise ValueError("Duplicate media printing face")
        seen.add(key)
        parent = prints[printing]
        if face not in [object_value(f)["face_id"] for f in array(parent["faces"])]:
            raise ValueError("Media face absent from printing")
        int_id, ordinal = integer(parent["int_id"]), integer(faces[face]["ordinal"])
        image_path(int_id, ordinal, "card_s")
        asset = images[image]
        active = (
            asset["publication_state"] == "approved"
            and asset["availability"] == "available"
        )
        sizes = (
            sorted(variants.get(image, []), key=lambda r: string(r["size_key"]))
            if active
            else []
        )
        binding: JsonValue = [image, int_id, ordinal]
        fingerprints, versions = _tokens(
            sizes,
            binding,
            active,
            object_value(members[key]) if key in members else {},
            revision,
            integer(old["revision"]) if old else 0,
        )
        next_members[key] = {
            "active": active,
            "binding": binding,
            **fingerprints,
            **versions,
        }
        result.append(
            row
            | {
                f: asset[f]
                for f in ("publication_state", "availability", "withdrawal_reason")
            }
            | versions
            | {
                "variants": [
                    {f: r[f] for f in ("size_key", "width", "height")} for r in sizes
                ]
            }
        )
        for variant in sizes:
            path = image_path(int_id, ordinal, string(variant["size_key"]))
            if path in paths:
                raise ValueError("Duplicate permanent image path")
            paths.add(path)
            assets.append(
                {
                    "path": path,
                    "source": variant["path"],
                    "sha256": "sha256:" + PurePosixPath(string(variant["path"])).stem,
                    **{f: variant[f] for f in ("width", "height", "bytes")},
                }
            )
    view = projection.tables | {
        "printing_image": result,
        "image_variant": [
            {k: v for k, v in row.items() if k != "path"}
            for row in projection.tables["image_variant"]
        ],
    }
    return MediaPlan(
        replace(projection, tables=view),
        {"revision": revision, "members": next_members},
        tuple(sorted(assets, key=lambda a: string(a["path"]))),
    )
