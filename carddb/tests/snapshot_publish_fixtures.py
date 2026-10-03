"""In-memory conditional fake S3/CDN and module-scoped synthetic image inputs."""

from contextlib import contextmanager
from copy import deepcopy
from dataclasses import replace
from datetime import UTC, datetime, timedelta
from io import BytesIO
from shutil import copytree
from threading import RLock
from typing import TYPE_CHECKING
from urllib.parse import urlsplit

import pytest
from PIL import Image

from sve_carddb.image_variants import build_variants
from sve_carddb.snapshot.export import Batch, export_snapshot
from sve_carddb.snapshot.media import prepare_media
from sve_carddb.snapshot.profiles import MEDIA
from sve_carddb.snapshot.publish import Ledger, Release
from sve_carddb.snapshot.publish.plan import prepare
from sve_carddb.snapshot.publish.storage import Stored
from sve_carddb.snapshot.values import (
    canonical,
    digest,
    integer,
    object_value,
    parse,
    string,
)

from .test_image_variants import source
from .test_snapshot_preview_images import PublicImages
from .test_snapshot_preview_images import images as images  # ruff: ignore[useless-import-alias] -- share one decoded synthetic library per module

if TYPE_CHECKING:
    from collections.abc import Callable, Iterator
    from pathlib import Path

    from pydantic import JsonValue

    from sve_carddb.snapshot.export import Brotli
    from sve_carddb.snapshot.project import Projection


class FakeS3:
    def __init__(self) -> None:
        self.objects: dict[str, Stored] = {}
        self.operations: list[tuple[str, str, str | None]] = []
        self.sequence = 0
        self.lock = RLock()
        self.leases = 0
        self.before_put: Callable[[str], None] | None = None
        self.before_delete: Callable[[str], None] | None = None
        self.before_get: Callable[[str], None] | None = None

    @contextmanager
    def exclusive(self) -> Iterator[None]:
        with self.lock:
            self.leases += 1
            try:
                yield
            finally:
                self.leases -= 1

    def get(self, key: str) -> Stored | None:
        if self.before_get is not None:
            self.before_get(key)
        return self.objects.get(key)

    def put(
        self, key: str, raw: bytes, headers: dict[str, str], *, expected: str | None
    ) -> bool:
        if self.before_put is not None:
            self.before_put(key)
        self.operations.append(("put", key, expected))
        old = self.objects.get(key)
        if (None if old is None else old.etag) != expected:
            return False
        self.sequence += 1
        self.objects[key] = Stored(raw, f'"fake-{self.sequence}"', headers.copy())
        return True

    def delete(self, key: str, *, expected: str) -> bool:
        if self.before_delete is not None:
            self.before_delete(key)
        self.operations.append(("delete", key, expected))
        old = self.objects.get(key)
        if old is None or old.etag != expected:
            return False
        del self.objects[key]
        return True

    def keys(self, prefix: str) -> tuple[str, ...]:
        return tuple(sorted(k for k in self.objects if k.startswith(prefix)))


class FakeCDN:
    def __init__(self, store: FakeS3, *, ignore_query: bool = False) -> None:
        self.store = store
        self.ignore_query = ignore_query
        self.cache: dict[str, bytes | None] = {}
        self.requests: list[str] = []

    def get(self, url: str) -> bytes | None:
        self.requests.append(url)
        split = urlsplit(url)
        key = split.path.lstrip("/")
        cache_key = key if self.ignore_query else url
        if cache_key not in self.cache:
            obj = self.store.get(key)
            self.cache[cache_key] = None if obj is None else obj.raw
        return self.cache[cache_key]


@pytest.fixture
def ledger(tmp_path: Path) -> Ledger:
    value = Ledger(tmp_path / "state", tmp_path / "backup")
    value.initialize()
    return value


@pytest.fixture(scope="module")
def changed_images(
    images: PublicImages, tmp_path_factory: pytest.TempPathFactory
) -> PublicImages:
    root = tmp_path_factory.mktemp("changed-publish-images")
    stream = BytesIO()
    Image.new("RGB", (160, 224), (200, 30, 70)).save(stream, format="PNG")
    output = build_variants(
        source(stream.getvalue()), blob_root=root / "library", cache_root=root / "cache"
    )
    view = deepcopy(images.projection)
    view.tables["image_variant"] = [
        {
            "image_id": "image",
            "size_key": v.size_key,
            "format": v.format,
            "path": v.path,
            "width": v.width,
            "height": v.height,
            "bytes": v.bytes,
        }
        for v in sorted(output.variants, key=lambda v: v.size_key)
    ]
    return PublicImages(view, images.ownership, root / "library")


@pytest.fixture(scope="module")
def mixed_images(
    images: PublicImages,
    changed_images: PublicImages,
    tmp_path_factory: pytest.TempPathFactory,
) -> PublicImages:
    """One changed art size forces both new art URLs, including identical bytes."""
    root = tmp_path_factory.mktemp("mixed-publish-images")
    copytree(images.library, root, dirs_exist_ok=True)
    copytree(changed_images.library, root, dirs_exist_ok=True)
    view = deepcopy(images.projection)
    replacement = next(
        v
        for v in changed_images.projection.tables["image_variant"]
        if v["size_key"] == "art_s"
    )
    view.tables["image_variant"] = [
        deepcopy(replacement if v["size_key"] == "art_s" else v)
        for v in view.tables["image_variant"]
    ]
    return PublicImages(view, images.ownership, root)


def candidate(
    ledger: Ledger,
    images: PublicImages,
    *,
    projection: Projection | None = None,
    from_version: str | None = None,
    brotli: Brotli | None = None,
) -> Release:
    next_number = integer(ledger.read()["high_water"]) + 1
    moment = datetime(2026, 10, 4, 1, 2, 3, tzinfo=UTC) + timedelta(seconds=next_number)
    version = moment.strftime("%Y%m%dT%H%M%SZ") + "-0001"
    revision = ledger.reserve(version)
    view = deepcopy(projection or images.projection)
    events = object_value(ledger.read()["first_events"])
    for event in view.tables["identity_change"]:
        event["data_version"] = events.get(string(event["id"]), version)
    media = prepare_media(
        view,
        images.library,
        revision=revision,
        previous=ledger.media_basis(),
    )
    snapshot = export_snapshot(
        media.projection,
        images.ownership,
        Batch("preview-" + version, moment.strftime("%Y-%m-%dT%H:%M:%SZ"), ("jp",)),
        format_version=MEDIA,
        brotli=brotli,
    )
    manifest = snapshot.manifest | {"data_version": version}
    changes = None
    if from_version is not None:
        changes = canonical(
            {
                "format_version": MEDIA,
                "from_data_version": from_version,
                "to_data_version": version,
                **{
                    k: []
                    for k in (
                        "added",
                        "modified",
                        "retired",
                        "errata",
                        "new_qa_versions",
                        "identity_changes",
                        "coverage_changes",
                        "support_changes",
                    )
                },
            }
        )
        from sve_carddb.snapshot.export.compression import compress  # ruff: ignore[import-outside-top-level] -- optional synthetic attachment

        blob = compress(changes, brotli)
        manifest["changes_ref"] = {
            "path": "snapshots/blobs/" + digest(changes)[7:] + ".json",
            "sha256": digest(changes),
            "bytes": len(changes),
            "compressed_bytes": {
                "gzip": len(blob.gzip),
                "br": None if blob.br is None else len(blob.br),
            },
        }
    snapshot = replace(snapshot, manifest=manifest)
    return prepare(
        snapshot,
        media,
        images.library,
        cdn_root="https://cdn.invalid/",
        changes=changes,
        brotli=brotli,
    )


def index(store: FakeS3) -> dict[str, JsonValue]:
    return object_value(parse(store.objects["snapshots/versions/index.json"].raw))


def version(release: Release) -> str:
    return string(release.entry["data_version"])
