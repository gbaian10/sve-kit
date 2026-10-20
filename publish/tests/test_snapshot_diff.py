"""Manifest differences over a fake SDK, including representation and recovery cases."""

from copy import deepcopy
from pathlib import Path
from time import perf_counter
from typing import TYPE_CHECKING, override

import pytest
from sve_carddb.export.read_api import (
    INDEX,
    POINTER,
    ExportError,
    array,
    canonical,
    closure,
    digest,
    integer,
    load_export,
    object_value,
    parse,
    string,
)
from sve_carddb.export.transport.compression import python_brotli

from sve_publish.adapter import SNAPSHOT_PREFIXES, Stored
from sve_publish.gc import collect, retained
from sve_publish.headers import INDEX_HEADERS, JSON_HEADERS, member_headers
from sve_publish.inventory import INVENTORY
from sve_publish.publish import upload
from sve_publish.snapshot_diff import missing_members

from .r2_export_fixtures import export
from .r2_export_fixtures import images as images  # ruff: ignore[useless-import-alias] -- synthetic corpus
from .r2_export_fixtures import roots as roots  # ruff: ignore[useless-import-alias] -- isolated export roots
from .r2_parallel_fixtures import DelayedS3, memory_store, ready

if TYPE_CHECKING:
    import httpx
    from pydantic import JsonValue
    from sve_carddb.export.preview import Roots
    from sve_carddb.export.read_api import Export

    from .synthetic_images import PublicImages

pytestmark = pytest.mark.usefixtures("close_sdk_clients")


def manifest(loaded: Export) -> dict[str, JsonValue]:
    return object_value(
        parse(
            next(
                m.raw for m in loaded.members if m.key == loaded.entry["manifest_path"]
            )
        )
    )


def declared(loaded: Export) -> set[str]:
    return closure(string(loaded.entry["manifest_path"]), manifest(loaded))


def retained_entry(
    fake: DelayedS3, entry: dict[str, JsonValue], value: dict[str, JsonValue]
) -> dict[str, JsonValue]:
    raw = canonical(value)
    result = entry | {
        "manifest_sha256": digest(raw),
        "manifest_path": "snapshots/manifests/" + digest(raw)[7:] + ".json",
    }
    fake.objects[string(result["manifest_path"])] = Stored(
        raw, '"retained"', JSON_HEADERS
    )
    return result


@pytest.mark.parametrize("slot", ["current", "previous"])
def test_old_manifest_shape_supports_incremental_upload_and_gc(
    images: PublicImages, roots: Roots, slot: str
) -> None:
    export(images, roots, step=0, brotli=python_brotli())
    first = load_export(roots.preview)
    fake = DelayedS3(delay=0)
    old = manifest(first)
    old["errata_card_ids"] = []
    old["min_reader_version"] = "4.0.0"
    old_entry = retained_entry(fake, first.entry | {"min_reader_version": "4.0.0"}, old)
    export(images, roots, step=1, brotli=python_brotli())
    second = load_export(roots.preview)
    current_entry = retained_entry(fake, second.entry, manifest(second))
    index: dict[str, JsonValue] = {
        "index_format": 2,
        "revision": 2,
        "current": old_entry if slot == "current" else current_entry,
        "previous": old_entry if slot == "previous" else None,
    }
    fake.objects[INDEX] = Stored(canonical(index), '"index"', INDEX_HEADERS)
    old_keys = closure(string(old_entry["manifest_path"]), old)
    known = old_keys | (declared(second) if slot == "previous" else set())
    with memory_store(fake) as store:
        missing = missing_members(store, index, second.members)
        assert {member.key for member in missing} == {
            member.key for member in second.members
        } - known
        _remote, required, optional = retained(store)
    assert required == known
    assert string(old_entry["manifest_path"]) + ".br" in optional
    assert all(method == "GET" for method, _key in fake.requests)


def test_new_export_rejects_the_old_errata_shape(
    images: PublicImages, roots: Roots
) -> None:
    export(images, roots, step=0)
    loaded = load_export(roots.preview)
    value = manifest(loaded)
    value["errata_card_ids"] = []
    raw = canonical(value)
    path = "snapshots/manifests/" + digest(raw)[7:] + ".json"
    (roots.preview / path).write_bytes(raw)
    (roots.preview / POINTER).write_bytes(
        canonical({"manifest_path": path, "manifest_sha256": digest(raw)})
    )
    with pytest.raises(ExportError, match="Export validation failed"):
        load_export(roots.preview)


@pytest.mark.parametrize(
    ("damage", "message"),
    [
        ("hash", "GC requires intact retained manifests"),
        ("canonical", "Manifest differs from its index entry"),
        ("files", "GC retained snapshot is invalid"),
        ("changes_ref", "GC retained snapshot is invalid"),
        ("format", "Unsupported retained snapshot format"),
    ],
)
def test_damaged_retained_envelopes_stop_gc_before_deletion(
    images: PublicImages, roots: Roots, damage: str, message: str
) -> None:
    export(images, roots, step=0)
    loaded = load_export(roots.preview)
    value = manifest(loaded)
    value["errata_card_ids"] = []
    if damage in {"files", "changes_ref"}:
        value[damage] = "invalid"
    elif damage == "format":
        value["format_version"] = "5.0.0"
    fake = DelayedS3(delay=0)
    identity = loaded.entry | (
        {"format_version": "5.0.0"} if damage == "format" else {}
    )
    entry = retained_entry(fake, identity, value)
    path = string(entry["manifest_path"])
    if damage in {"hash", "canonical"}:
        raw = fake.objects[path].raw + b"\n"
        if damage == "canonical":
            path = "snapshots/manifests/" + digest(raw)[7:] + ".json"
            entry |= {"manifest_path": path, "manifest_sha256": digest(raw)}
        fake.objects[path] = Stored(raw, '"damaged"', JSON_HEADERS)
    fake.objects[INDEX] = Stored(
        canonical(
            {"index_format": 2, "revision": 1, "current": entry, "previous": None}
        ),
        '"index"',
        INDEX_HEADERS,
    )
    with memory_store(fake) as store, pytest.raises(ExportError, match=message):
        collect(store, SNAPSHOT_PREFIXES, execute=True)
    assert all(method == "GET" for method, _key in fake.requests)


def test_first_publish_and_current_rerun_trust_the_committed_index(
    images: PublicImages, roots: Roots
) -> None:
    export(images, roots, step=0, brotli=python_brotli())
    loaded = load_export(roots.preview)
    fake = DelayedS3(delay=0)
    ready(fake, roots)
    with memory_store(fake) as store:
        first = upload(store, loaded)
        assert first["written_files"] == len(loaded.members)
        assert {key for method, key in fake.requests if method == "GET"} == {
            INDEX,
            INVENTORY,
        }
        assert fake.requests[-2:] == [("PUT", INDEX), ("GET", INDEX)]
        fake.requests.clear()
        again = upload(store, loaded)
    assert again["written_files"] == 0
    assert again["unchanged_files"] == len(loaded.members)
    assert again["index"] == "unchanged"
    assert all(
        method == "GET" and key in {INDEX, INVENTORY} for method, key in fake.requests
    )


def test_text_only_update_reuses_current_and_previous_without_image_io(
    images: PublicImages, roots: Roots, monkeypatch: pytest.MonkeyPatch
) -> None:
    export(images, roots, step=0, brotli=python_brotli())
    first = load_export(roots.preview)
    fake = DelayedS3()
    ready(fake, roots)
    with memory_store(fake) as store:
        upload(store, first)
        text = deepcopy(images.projection)
        text.tables["printing"][0]["rarity_raw"] = "Synthetic text correction"
        export(images, roots, step=1, projection=text, brotli=python_brotli())
        second = load_export(roots.preview)
        upload(store, second)
        export(images, roots, step=2, brotli=python_brotli())
        previous_only = declared(first) - declared(second)
        assert previous_only
        for path in (roots.preview / "images").rglob("*.webp"):
            path.unlink()

        original = Path.read_bytes

        def no_images(path: Path) -> bytes:
            assert path.suffix != ".webp", "snapshot touched a bitmap"
            return original(path)

        monkeypatch.setattr(Path, "read_bytes", no_images)
        started = perf_counter()
        third = load_export(roots.preview)
        validated = perf_counter()
        assert previous_only & declared(third)
        expected = {m.key for m in third.members} - declared(first) - declared(second)
        fake.requests.clear()
        result = upload(store, third)
        finished = perf_counter()
    writes = {key for method, key in fake.requests if method == "PUT"}
    reads = [key for method, key in fake.requests if method == "GET"]
    assert writes == expected | {INDEX}
    assert len(reads) == 7
    assert set(reads) == {
        INDEX,
        INVENTORY,
        string(first.entry["manifest_path"]),
        string(second.entry["manifest_path"]),
    }
    assert result["written_files"] == len(expected)
    assert fake.requests[-2:] == [("PUT", INDEX), ("GET", INDEX)]
    assert not any(key.startswith("images/") for _, key in fake.requests)
    print(  # ruff: ignore[print] -- acceptance evidence contains counts and synthetic timings only
        f"text-only fake SDK, 20ms/request: local={validated - started:.3f}s, "
        f"remote={finished - validated:.3f}s, total={finished - started:.3f}s; "
        f"GET={len(reads)}, PUT={len(writes)}, new JSON/br={len(expected)}, "
        f"image requests=0, bitmap reads=0"
    )


@pytest.mark.parametrize("old_br", [False, True])
def test_raw_and_brotli_are_independent_representations(
    images: PublicImages, roots: Roots, *, old_br: bool
) -> None:
    export(images, roots, step=0, brotli=python_brotli() if old_br else None)
    first = load_export(roots.preview)
    fake = DelayedS3(delay=0)
    ready(fake, roots)
    with memory_store(fake) as store:
        upload(store, first)
        export(images, roots, step=1, brotli=None if old_br else python_brotli())
        second = load_export(roots.preview)
        fake.requests.clear()
        upload(store, second)
    expected = {m.key for m in second.members} - declared(first)
    assert {key for method, key in fake.requests if method == "PUT"} == expected | {
        INDEX
    }
    reused_raw = declared(first) & declared(second)
    assert reused_raw
    if not old_br:
        assert any(key + ".br" in expected for key in reused_raw)
    else:
        assert all(m.key in fake.objects for m in first.members)
    assert not any(
        method == "GET" and key.endswith(".br") for method, key in fake.requests
    )


# Differing bytes and a 412 without an object are covered by the upload and parallel tests.
@pytest.mark.parametrize("same_headers", [True, False])
def test_only_a_create_only_collision_is_downloaded(
    images: PublicImages, roots: Roots, *, same_headers: bool
) -> None:
    export(images, roots, step=0, brotli=python_brotli())
    loaded = load_export(roots.preview)
    member = loaded.members[0]
    fake = DelayedS3(delay=0)
    ready(fake, roots)
    fake.objects[member.key] = Stored(
        member.raw, '"orphan"', member_headers(member) if same_headers else {}
    )
    with memory_store(fake) as store:
        if same_headers:
            assert upload(store, loaded)["unchanged_files"] == 1
        else:
            with pytest.raises(ExportError, match="Immutable JSON"):
                upload(store, loaded, jobs=1)
            assert INDEX not in fake.objects
    assert [
        key
        for method, key in fake.requests
        if method == "GET" and key not in {INDEX, INVENTORY}
    ] == [member.key]


def test_optional_manifest_brotli_is_not_inferred_from_its_raw_manifest(
    images: PublicImages, roots: Roots
) -> None:
    export(images, roots, step=0, brotli=python_brotli())
    first = load_export(roots.preview)
    fake = DelayedS3(delay=0)
    ready(fake, roots)
    with memory_store(fake) as store:
        upload(store, first)
        export(images, roots, step=1, brotli=python_brotli())
        second = load_export(roots.preview)
        sibling = string(second.entry["manifest_path"]) + ".br"
        fake.fail_key = INDEX
        with pytest.raises(ExportError, match="conditional write failed"):
            upload(store, second)
        del fake.objects[sibling]
        fake.fail_key = None
        fake.requests.clear()
        upload(store, second)
    assert ("PUT", sibling) in fake.requests
    assert ("GET", sibling) not in fake.requests
    assert ("PUT", string(second.entry["manifest_path"])) in fake.requests
    assert fake.objects[sibling].raw == next(
        m.raw for m in second.members if m.key == sibling
    )


@pytest.mark.parametrize(
    "damage",
    [
        "missing",
        "hash",
        "canonical",
        "format",
        "schema",
        "identity",
        "manifest-path",
        "path",
        "raw-length",
        "br-length",
    ],
)
def test_bad_retained_manifests_stop_before_any_put(
    images: PublicImages, roots: Roots, damage: str
) -> None:
    export(images, roots, step=0, brotli=python_brotli())
    first = load_export(roots.preview)
    fake = DelayedS3(delay=0)
    ready(fake, roots)
    with memory_store(fake) as store:
        upload(store, first)
        entry = dict(first.entry)
        raw = manifest(first)
        if damage == "schema":
            raw["files"] = "invalid"
        elif damage == "format":
            raw["format_version"] = entry["format_version"] = "5.0.0"
        elif damage == "identity":
            entry["required_capabilities"] = ["synthetic-capability"]
        elif damage in {"path", "raw-length", "br-length"}:
            row = object_value(array(raw["files"])[0])
            if damage == "path":
                row["path"] = "snapshots/blobs/" + "a" * 64 + ".json"
            elif damage == "raw-length":
                row["bytes"] = integer(row["bytes"]) + 1
            else:
                lengths = object_value(row["compressed_bytes"])
                lengths["br"] = integer(lengths["br"]) + 1
        content = canonical(raw)
        if damage == "canonical":
            content += b"\n"
        entry["manifest_sha256"] = digest(content)
        entry["manifest_path"] = "snapshots/manifests/" + digest(content)[7:] + ".json"
        if damage == "manifest-path":
            entry["manifest_path"] = "snapshots/manifests/" + "f" * 64 + ".json"
        key = string(entry["manifest_path"])
        if damage == "missing":
            fake.objects.pop(key, None)
        else:
            fake.objects[key] = Stored(
                b"broken" if damage == "hash" else content, '"damaged"', JSON_HEADERS
            )
        fake.objects[INDEX] = Stored(
            canonical(
                {"index_format": 2, "revision": 1, "current": entry, "previous": None}
            ),
            '"index"',
            INDEX_HEADERS,
        )
        export(images, roots, step=1, brotli=python_brotli())
        fake.requests.clear()
        with pytest.raises(
            ExportError, match=r"manifest|Manifest|length conflicts|payload path|format"
        ):
            upload(store, load_export(roots.preview))
    assert not any(method == "PUT" for method, _ in fake.requests)


@pytest.mark.parametrize("representation", ["raw", "br"])
def test_current_previous_descriptor_conflicts_stop_before_put(
    images: PublicImages, roots: Roots, representation: str
) -> None:
    export(images, roots, step=0, brotli=python_brotli())
    first = load_export(roots.preview)
    fake = DelayedS3(delay=0)
    ready(fake, roots)
    with memory_store(fake) as store:
        upload(store, first)
        export(images, roots, step=1, brotli=python_brotli())
        upload(store, load_export(roots.preview))
        old = manifest(first)
        row = object_value(array(old["files"])[0])
        if representation == "raw":
            row["bytes"] = integer(row["bytes"]) + 1
        else:
            lengths = object_value(row["compressed_bytes"])
            lengths["br"] = integer(lengths["br"]) + 1
        index = object_value(parse(fake.objects[INDEX].raw))
        index["previous"] = retained_entry(fake, first.entry, old)
        fake.objects[INDEX] = Stored(canonical(index), '"index"', INDEX_HEADERS)
        export(images, roots, step=2, brotli=python_brotli())
        fake.requests.clear()
        with pytest.raises(ExportError, match="length conflicts"):
            upload(store, load_export(roots.preview))
    assert not any(method == "PUT" for method, _ in fake.requests)


def test_changes_ref_retains_its_raw_and_brotli_without_payload_reads(
    images: PublicImages, roots: Roots
) -> None:
    export(images, roots, step=0, brotli=python_brotli())
    first = load_export(roots.preview)
    text = deepcopy(images.projection)
    text.tables["printing"][0]["rarity_raw"] = "Synthetic changes reference"
    export(images, roots, step=1, projection=text, brotli=python_brotli())
    second = load_export(roots.preview)
    # The old manifest can retain a payload through changes_ref alone.
    reference = next(
        object_value(row)
        for row in array(manifest(second)["files"])
        if object_value(row)["path"] not in declared(first)
    )
    old = manifest(first)
    old["changes_ref"] = {
        key: reference[key] for key in ("path", "sha256", "bytes", "compressed_bytes")
    }
    fake = DelayedS3(delay=0)
    ready(fake, roots)
    old_entry = retained_entry(fake, first.entry, old)
    fake.objects[INDEX] = Stored(
        canonical(
            {"index_format": 2, "revision": 1, "current": old_entry, "previous": None}
        ),
        '"index"',
        INDEX_HEADERS,
    )
    key = string(reference["path"])
    for member in second.members:
        if member.key in {key, key + ".br"}:
            fake.objects[member.key] = Stored(
                member.raw, '"retained"', member_headers(member)
            )
    with memory_store(fake) as store:
        upload(store, second)
    assert not any(path in {key, key + ".br"} for _, path in fake.requests)
    assert {path for method, path in fake.requests if method == "GET"} == {
        INDEX,
        INVENTORY,
        string(old_entry["manifest_path"]),
    }


@pytest.mark.parametrize("changed", ["images", "index", "readback"])
def test_control_changes_prevent_successful_publication(
    images: PublicImages, roots: Roots, changed: str
) -> None:
    export(images, roots, step=0)
    first = load_export(roots.preview)
    export(images, roots, step=1)
    second = load_export(roots.preview)
    trigger = string(second.entry["manifest_path"])

    class ChangingControl(DelayedS3):
        @override
        def respond(
            self, request: httpx.Request, key: str, existing: Stored | None
        ) -> httpx.Response:
            response = super().respond(request, key, existing)
            if request.method == "PUT" and key == (
                INDEX if changed == "readback" else trigger
            ):
                target = INVENTORY if changed == "images" else INDEX
                old = self.objects[target]
                self.objects[target] = Stored(
                    old_index.raw if changed == "readback" else old.raw,
                    '"concurrent"',
                    old.headers,
                )
            return response

    fake = ChangingControl(delay=0)
    ready(fake, roots)
    with memory_store(fake) as store:
        # Seed a committed baseline without activating the fault.
        fake.objects[INDEX] = Stored(
            canonical(
                {
                    "index_format": 2,
                    "revision": 1,
                    "current": first.entry,
                    "previous": None,
                }
            ),
            '"baseline"',
            INDEX_HEADERS,
        )
        old_index = fake.objects[INDEX]
        for member in first.members:
            fake.objects[member.key] = Stored(
                member.raw, '"baseline"', member_headers(member)
            )
        message = {
            "images": "inventory changed",
            "index": "index changed",
            "readback": "index read-back",
        }[changed]
        with pytest.raises(ExportError, match=message):
            upload(store, second, jobs=1)
    if changed != "readback":
        assert ("PUT", INDEX) not in fake.requests
