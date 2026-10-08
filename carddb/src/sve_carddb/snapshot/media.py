"""Verified 2.0 media projection and printing-face image revision comparison.

The caller supplies a new high-water revision and the last export's state.
This module does not allocate revisions or publish remote assets.
"""

import re
from dataclasses import dataclass, replace
from pathlib import Path, PurePosixPath
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.core.json import (
    array,
    canonical,
    digest,
    integer,
    object_value,
    parse,
    string,
)
from sve_carddb.snapshot.media_config import offline_configuration
from sve_carddb.snapshot.media_urls import _uint, image_path
from sve_carddb.snapshot.preview.images import image_blobs

if TYPE_CHECKING:
    from collections.abc import Iterator

    from sve_carddb.image_checks import ImageChecks
    from sve_carddb.snapshot.project import Projection
    from sve_carddb.snapshot.project.source import Record


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


_SUBJECT_ARITY = 2
_BINDING_ARITY = 3


def _member(member: Record, revision: int) -> None:
    if set(member) != {"binding", "card", "art", "card_version", "art_version"}:
        raise ValueError("member")
    binding = array(member["binding"])
    if len(binding) != _BINDING_ARITY or not string(binding[0]):
        raise ValueError("binding")
    _uint(integer(binding[1]), positive=True)
    _uint(integer(binding[2]), positive=False)
    for group in ("card", "art"):
        if re.fullmatch(r"sha256:[0-9a-f]{64}", string(member[group])) is None:
            raise ValueError("fingerprint")
        token = _uint(integer(member[group + "_version"]), positive=True)
        if token > revision:
            raise ValueError("token")


def _checked_state(value: JsonValue) -> Record:
    old = object_value(value)
    if set(old) != {"revision", "members"}:
        raise ValueError("fields")
    revision = _uint(integer(old["revision"]), positive=True)
    for key, raw in object_value(old["members"]).items():
        ids = array(parse(key.encode()))
        if (
            len(ids) != _SUBJECT_ARITY
            or not all(isinstance(i, str) and i for i in ids)
            or canonical(ids).decode() != key
        ):
            raise ValueError("identity")
        _member(object_value(raw), revision)
    return old


def _state(value: JsonValue) -> Record:
    if value is None:
        return {}
    try:
        result = _checked_state(value)
    except (ValueError, KeyError, TypeError) as error:
        raise ValueError("Invalid committed media state") from error
    else:
        return result


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
    checks: ImageChecks | None = None,
) -> MediaPlan:
    """Verify private content-addressed outputs, then compare each card/art group.

    The state keeps only this export's available members, so a removed or
    restored image never matches and always receives the new revision.
    """
    _uint(revision, positive=True)
    old = _state(previous)
    if old and (
        set(old) != {"revision", "members"} or revision <= integer(old["revision"])
    ):
        raise ValueError("Media revision must advance the committed state")
    members = {} if not old else object_value(old["members"])
    # Decode each unique source once before accepting its filename digest as evidence.
    for _path, _raw in image_blobs(projection.tables, source, checks=checks):
        pass
    prints = {string(r["id"]): r for r in projection.tables["printing"]}
    faces = {string(r["id"]): r for r in projection.tables["face"]}
    variants: dict[str, list[Record]] = {}
    for row in projection.tables["image_variant"]:
        variants.setdefault(string(row["image_id"]), []).append(row)
    result: list[Record] = []
    assets: list[dict[str, JsonValue]] = []
    next_members: dict[str, JsonValue] = {}
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
        active = (
            row["publication_state"] == "approved"
            and row["availability"] == "available"
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
        if active:
            next_members[key] = {"binding": binding, **fingerprints, **versions}
        result.append(
            row
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
        replace(
            projection, tables=view, config=offline_configuration(projection.config)
        ),
        {"revision": revision, "members": next_members},
        tuple(sorted(assets, key=lambda a: string(a["path"]))),
    )
