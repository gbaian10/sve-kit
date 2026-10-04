"""Small immutable public template and a conditional in-memory S3 service."""

import gzip
import hashlib
import hmac
import shutil
from dataclasses import dataclass, field, replace
from datetime import UTC, datetime
from typing import TYPE_CHECKING

import httpx
import pytest

from sve_carddb.r2_upload.plan import Plan, plan_preview
from sve_carddb.r2_upload.s3 import S3, Credentials
from sve_carddb.snapshot.export import export_snapshot
from sve_carddb.snapshot.preview import Roots, write_preview
from sve_carddb.snapshot.values import canonical, digest, string

from .r2_sdk_fixtures import mock_client
from .test_snapshot_export import BATCH
from .test_snapshot_preview_images import images as images  # ruff: ignore[useless-import-alias] -- register the shared synthetic image/export fixture

if TYPE_CHECKING:
    from collections.abc import Callable
    from pathlib import Path

    from sve_carddb.snapshot.project.source import Record

    from .test_snapshot_preview_images import PublicImages

ACCOUNT = "0" * 32
BUCKET = "synthetic-preview"
CREDENTIALS = Credentials("synthetic-access", "synthetic-secret")
NOW = datetime(2026, 10, 2, 0, 0, tzinfo=UTC)


def verify_signature(request: httpx.Request) -> None:
    # Build the server's canonical request independently from the production signer.
    algorithm, fields = request.headers["authorization"].split(" ", 1)
    parsed = dict(part.split("=", 1) for part in fields.split(", "))
    credential, scope = parsed["Credential"].split("/", 1)
    assert credential == CREDENTIALS.access_key
    assert scope.endswith("/auto/s3/aws4_request")
    names = parsed["SignedHeaders"].split(";")
    assert names == sorted(names)
    assert "host" in names
    assert "x-amz-content-sha256" in names
    if request.method == "PUT":
        assert "if-match" in names or "if-none-match" in names
        assert "content-type" in names
        assert "cache-control" in names
    payload = hashlib.sha256(request.content).hexdigest()
    assert payload == request.headers["x-amz-content-sha256"]
    headers = "".join(
        f"{key}:{' '.join(request.headers[key].split())}\n" for key in names
    )
    canonical = f"{request.method}\n{request.url.path}\n\n{headers}\n{';'.join(names)}\n{payload}"
    to_sign = f"{algorithm}\n{request.headers['x-amz-date']}\n{scope}\n{hashlib.sha256(canonical.encode()).hexdigest()}"
    signing_key = b"AWS4" + CREDENTIALS.secret_key.encode()
    for value in scope.split("/"):
        signing_key = hmac.new(signing_key, value.encode(), hashlib.sha256).digest()
    assert (
        hmac.new(signing_key, to_sign.encode(), hashlib.sha256).hexdigest()
        == parsed["Signature"]
    )


@dataclass
class Store:
    objects: dict[str, tuple[bytes, dict[str, str]]] = field(default_factory=dict)
    calls: list[tuple[str, str]] = field(default_factory=list)
    intervene: Callable[[httpx.Request, str], httpx.Response | None] | None = None
    generation: int = 0

    def save(self, key: str, raw: bytes, headers: dict[str, str]) -> None:
        self.generation += 1
        self.objects[key] = raw, headers | {"etag": f'"generation-{self.generation}"'}

    def handle(self, request: httpx.Request) -> httpx.Response:
        assert request.url.scheme == "https"
        assert request.url.host == ACCOUNT + ".r2.cloudflarestorage.com"
        assert request.headers["accept-encoding"] == "identity"
        verify_signature(request)
        key = request.url.path.removeprefix("/" + BUCKET + "/")
        self.calls.append((request.method, key))
        if self.intervene is not None:
            response = self.intervene(request, key)
            if response is not None:
                return response
        if request.method == "GET":
            if key not in self.objects:
                return httpx.Response(404)
            raw, headers = self.objects[key]
            return httpx.Response(200, content=raw, headers=headers)
        assert request.method == "PUT"
        if "if-none-match" in request.headers:
            assert request.headers["if-none-match"] == "*"
            if key in self.objects:
                return httpx.Response(412)
        else:
            assert "if-match" in request.headers
            if (
                key not in self.objects
                or request.headers["if-match"] != self.objects[key][1]["etag"]
            ):
                return httpx.Response(412)
        self.save(
            key,
            request.content,
            {k: request.headers[k] for k in ("content-type", "cache-control")},
        )
        return httpx.Response(200)

    def remote(self) -> S3:
        return S3(
            ACCOUNT,
            BUCKET,
            CREDENTIALS,
            mock_client(
                httpx.Client(transport=httpx.MockTransport(self.handle)),
                account=ACCOUNT,
                bucket=BUCKET,
                credentials=CREDENTIALS,
            ),
            lambda _delay: None,
        )


@pytest.fixture(scope="module")
def public_template(
    images: PublicImages, tmp_path_factory: pytest.TempPathFactory
) -> Path:
    parent = tmp_path_factory.mktemp("r2-public-template")
    roots = Roots(parent / "preview", parent / "formal")
    write_preview(images.snapshot(), roots, {}, image_source=images.library)
    (roots.preview / "private").mkdir()
    (roots.preview / "private/recipe.json").write_text('"never read or upload"')
    return roots.preview


@pytest.fixture(scope="module")
def public_plan(public_template: Path) -> Plan:
    return plan_preview(public_template)


@pytest.fixture
def local(public_plan: Plan, tmp_path: Path) -> Plan:
    root = tmp_path / "preview"
    shutil.copytree(public_plan.root, root)
    return replace(public_plan, root=root)


def synthetic_transport(
    images: PublicImages,
    tables: dict[str, list[Record]],
    root: Path,
    replacements: dict[str, bytes] | None = None,
) -> Path:
    # Bypass the writer's image checks so the uploader must independently reject damaged metadata.
    snapshot = images.snapshot(tables)
    manifest = canonical(snapshot.manifest)
    raw_files = {
        digest(b.raw): b.raw for b in [*snapshot.payloads.values(), snapshot.text_all]
    }
    raw_files[digest(manifest)] = manifest
    for hashed, raw in raw_files.items():
        kind = "manifests" if raw == manifest else "blobs"
        target = root / f"snapshots/{kind}/{hashed[7:]}.json"
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(raw)
        target.with_suffix(".json.gz").write_bytes(gzip.compress(raw, mtime=0))
    pointer = root / "snapshots/preview/current.json"
    pointer.parent.mkdir(parents=True)
    pointer.write_bytes(
        canonical(
            {
                "manifest_path": f"snapshots/manifests/{digest(manifest)[7:]}.json",
                "manifest_sha256": digest(manifest),
            }
        )
    )
    for row in tables["image_variant"]:
        key = string(row["path"])
        target = root / key
        target.parent.mkdir(parents=True, exist_ok=True)
        replacement = (replacements or {}).get(key)
        target.write_bytes(
            replacement
            if replacement is not None
            else (images.library / key).read_bytes()
        )
    return root


@pytest.fixture(scope="module")
def previous(images: PublicImages, tmp_path_factory: pytest.TempPathFactory) -> Plan:
    parent = tmp_path_factory.mktemp("r2-previous-version")
    tables = images.tables()
    tables["image_asset"][0]["availability"] = "missing"
    tables["image_variant"] = []
    tables["printing"][0]["card_no"] = "SYNTHETIC-OLD"
    projection = replace(images.projection, tables=tables)
    snapshot = export_snapshot(
        projection,
        images.ownership,
        replace(
            BATCH,
            data_version="preview-20261002T000001Z-0001",
            published_at="2026-10-02T00:00:01Z",
        ),
    )
    roots = Roots(parent / "preview", parent / "formal")
    write_preview(snapshot, roots, {})
    return plan_preview(roots.preview)
