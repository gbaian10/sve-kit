"""Current sealed-source coverage and exact position counterexamples."""

import copy
import re
import shutil
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.manifest import Kind
from sve_carddb.snapshot.values import digest
from sve_carddb.source_archive import ArchiveError, Scope, seal_batch
from sve_carddb.sources.official_jp import card_url
from sve_carddb.template_sources import inventory, pins
from sve_carddb.template_sources.inventory import coverage, fields, replay, scan_current
from sve_carddb.template_sources.normalizer import partition
from sve_carddb.translations.sources import project

from .template_source_fixtures import template_case as template_case  # ruff: ignore[useless-import-alias] -- register reusable synthetic inputs
from .test_effect_presence import page
from .test_source_archive import _put, _resource, _store

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.catalog.adoption_models import SourceRef
    from sve_carddb.template_sources.models import Entry
    from sve_carddb.template_sources.normalizer import Part

    from .template_source_fixtures import Case


def test_every_candidate_replays_the_pinned_complete_field(template_case: Case) -> None:
    sources = FrozenSources(template_case.store, "test-store", template_case.batch)
    documents = {}
    for current in sources.inventory.current:
        source, raw, _ = sources.read(
            current.source_version_id, parser_version=pins.PARSER
        )
        documents[source.id] = project(raw, source.url, "jp")[1]
    for item in template_case.scan.entries:
        assert (
            digest(
                replay(
                    item,
                    documents[item.source_ref.source_version_id],
                    store_id="test-store",
                ).encode()
            )
            == item.normalized_hash
        )
        assert item.legacy_fingerprint == (
            item.normalized_hash if item.role == "body" else None
        )


@pytest.mark.parametrize(
    "change",
    [
        "hash",
        "legacy_hash",
        "line",
        "role",
        "id",
        "normalizer",
        "parser",
        "field_hash",
        "nontext",
        "nonability_field",
    ],
)
def test_entry_replay_rejects_changed_hash_or_coordinates(
    template_case: Case, change: str
) -> None:
    item = next(item for item in template_case.scan.entries if item.role == "body")
    sources = FrozenSources(template_case.store, "test-store", template_case.batch)
    source, raw, _ = sources.read(
        item.source_ref.source_version_id, parser_version=pins.PARSER
    )
    document = project(raw, source.url, "jp")[1]
    message = "Template entry cannot be replayed from the pinned field recipe"
    if change in {"hash", "legacy_hash", "line", "role", "id", "normalizer"}:
        name = {
            "hash": "normalized_hash",
            "legacy_hash": "legacy_fingerprint",
            "line": "line_ordinal",
            "normalizer": "normalizer_id",
        }.get(change, change)
        values: dict[str, JsonValue] = {
            "hash": digest(b"wrong"),
            "legacy_hash": None,
            "line": 50,
            "role": "reminder",
            "id": "wrong",
            "normalizer": "unknown",
        }
        item = item.model_copy(update={name: values[change]})
        if change == "normalizer":
            message = "Template entry uses an unsupported recipe"
    elif change == "parser":
        item = item.model_copy(
            update={
                "source_ref": item.source_ref.model_copy(update={"parser": "unknown"})
            }
        )
        message = "Template entry uses an unsupported recipe"
    elif change == "field_hash":
        item = item.model_copy(
            update={
                "source_ref": item.source_ref.model_copy(
                    update={"text_hash": digest(b"wrong")}
                )
            }
        )
        message = "Template entry must locate exact hash-verified text"
    elif change == "nontext":
        item = item.model_copy(
            update={
                "source_ref": item.source_ref.model_copy(update={"locator": "/faces"})
            }
        )
        message = "Template entry must locate exact hash-verified text"
    else:
        ref = item.source_ref.model_copy(
            update={"locator": "/faces/0/name", "text_hash": digest(b"Synthetic name")}
        )
        part = partition("Synthetic name")[0]
        item = inventory.entry(ref, part, item.normalizer_id, store_id="test-store")
    with pytest.raises(ValueError, match=rf"\A{re.escape(message)}\Z"):
        replay(item, document, store_id="test-store")


@pytest.mark.parametrize(
    "change",
    [
        "unknown_header",
        "unknown_presence",
        "parse_failure",
        "no_effect",
        "empty_effect",
    ],
)
def test_refused_sources_are_never_silently_dropped(
    tmp_path: Path, change: str
) -> None:
    store = _store(tmp_path / "source")
    effect = {
        "unknown_header": '<div class="detail">Alpha２<br>-----<br>『Private synthetic name』{Synthetic}UnknownType</div>',
        "unknown_presence": '<div class="detail">Alpha２</div>',
        "parse_failure": "",
        "no_effect": "",
        "empty_effect": '<div class="detail"></div>',
    }[change]
    raw = page("jp", effect)
    if change == "unknown_presence":
        raw = raw.removesuffix(b"</body></html>")
    elif change == "parse_failure":
        raw = raw.replace(b'class="ttl"', b'class="unknown"')
    _put(store, _resource(card_url("SYN-01"), "raw/card.html", raw, Kind.CARD), raw)
    batch = seal_batch(store)
    scan = scan_current(
        FrozenSources(store.root, store.store_id, batch.batch_id),
    )
    assert coverage(scan)["complete"] is (change in {"no_effect", "empty_effect"})
    if change in {"unknown_header", "unknown_presence", "parse_failure"}:
        reason = {
            "unknown_header": "unrecognized_token_header",
            "unknown_presence": "unknown_effect_presence",
            "parse_failure": "jp_projection_failed",
        }[change]
        assert [value["reason"] for value in scan.failures] == [reason]
    else:
        assert scan.fields[0]["state"] == (
            "absent" if change == "no_effect" else "empty"
        )


def test_corrupt_archived_raw_is_not_replaced_by_live_or_latest_data(
    template_case: Case, tmp_path: Path
) -> None:
    root = tmp_path / "archive"
    shutil.copytree(template_case.store, root)
    corrupted = next((root / "raw").rglob("*.raw"))
    corrupted.write_bytes(b"Synthetic altered raw")
    message = f"archive file hash or size mismatch: {corrupted}"
    with pytest.raises(ArchiveError, match=rf"\A{re.escape(message)}\Z"):
        FrozenSources(root, "test-store", template_case.batch)


def test_wrong_batch_scope_is_rejected(template_case: Case) -> None:
    sources = FrozenSources(template_case.store, "test-store", template_case.batch)
    sources.inventory.scope = [Scope(provider="en", kind="card")]
    with pytest.raises(
        ValueError,
        match=r"\ATemplate checkpoint requires an exclusively JP card batch\Z",
    ):
        scan_current(sources)


def test_nonability_projection_and_missing_faces_are_not_sources() -> None:
    with pytest.raises(
        ValueError, match=r"\ATemplate projection must contain card faces\Z"
    ):
        fields({"faces": []})
    with pytest.raises(
        ValueError, match=r"\ATemplate field must be exact text or unknown\Z"
    ):
        fields({"faces": [{"text": 3, "sections": []}]})
    with pytest.raises(
        TypeError, match=r"\ATemplate section must contain exact text\Z"
    ):
        fields({"faces": [{"text": None, "sections": [None]}]})


def test_jp_card_batch_with_nonpage_media_is_refused(tmp_path: Path) -> None:
    store = _store(tmp_path / "source")
    raw = page("jp", '<div class="detail">Synthetic２</div>')
    resource = replace(
        _resource(card_url("SYN-01"), "raw/card.html", raw, Kind.CARD),
        content_type="image/png",
    )
    _put(store, resource, raw)
    batch = seal_batch(store)
    with pytest.raises(
        ValueError, match=r"\ATemplate frozen source identity or media mismatch\Z"
    ):
        scan_current(
            FrozenSources(store.root, store.store_id, batch.batch_id),
        )


def test_candidate_id_collision_is_refused_before_publication(
    template_case: Case, monkeypatch: pytest.MonkeyPatch
) -> None:
    original = inventory.entry

    def colliding_entry(
        ref: SourceRef, part: Part, normalizer_id: str, *, store_id: str
    ) -> Entry:
        return original(ref, part, normalizer_id, store_id=store_id).model_copy(
            update={"id": "inv:synthetic-collision"}
        )

    monkeypatch.setattr(inventory, "entry", colliding_entry)
    with pytest.raises(
        ValueError, match=r"\ATemplate inventory entry IDs must be unique\Z"
    ):
        scan_current(
            FrozenSources(template_case.store, "test-store", template_case.batch),
        )


def test_projection_number_type_is_checked_at_the_boundary(
    template_case: Case, monkeypatch: pytest.MonkeyPatch
) -> None:
    def malformed_project(
        _raw: bytes, _url: str, _provider: str
    ) -> tuple[str, JsonValue]:
        return "ja", {"number": 3, "faces": [{"text": "Synthetic", "sections": []}]}

    monkeypatch.setattr(inventory, "project", malformed_project)
    with pytest.raises(TypeError, match=r"\ATemplate projection lacks a card number\Z"):
        scan_current(
            FrozenSources(template_case.store, "test-store", template_case.batch),
        )


def test_section_count_type_is_checked_before_computing_expected_fields(
    template_case: Case,
) -> None:
    scan = copy.deepcopy(template_case.scan)
    scan.pages[0]["section_counts"] = ["1"]
    with pytest.raises(
        TypeError, match=r"\ATemplate coverage section count must be an integer\Z"
    ):
        coverage(scan)
