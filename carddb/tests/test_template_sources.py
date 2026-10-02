"""Sealed source, immutable recipe, replay and output-boundary counterexamples."""

import argparse
import copy
import re
import shutil
import sys
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.manifest import Kind
from sve_carddb.registry.storage import MAX_BYTES, read_yaml
from sve_carddb.snapshot.values import canonical, digest, object_value, parse
from sve_carddb.source_archive import ArchiveError, Scope, seal_batch
from sve_carddb.sources.official_jp import card_url
from sve_carddb.template_sources import __main__ as command
from sve_carddb.template_sources import inventory, output, pins
from sve_carddb.template_sources.inventory import coverage, fields, replay, scan_batch
from sve_carddb.template_sources.models import Inventory
from sve_carddb.template_sources.normalizer import partition
from sve_carddb.translations.sources import project

from .adoption_fixtures import commit
from .template_source_fixtures import template_case as template_case  # ruff: ignore[useless-import-alias] -- register reusable synthetic inputs
from .test_effect_presence import page
from .test_source_archive import _put, _resource, _store

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.catalog.adoption_models import SourceRef
    from sve_carddb.template_sources.inventory import Scan
    from sve_carddb.template_sources.models import Entry, Recipe
    from sve_carddb.template_sources.normalizer import Part

    from .template_source_fixtures import Case


def arguments(case: Case, destination: Path) -> argparse.Namespace:
    return argparse.Namespace(
        store=case.store,
        store_id="test-store",
        batch_id=case.batch,
        repository=case.repository,
        code_revision=case.revision,
        legacy=case.legacy_path,
        expected_templates=3,
        output=destination,
    )


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
            digest(replay(item, documents[item.source_ref.source_version_id]).encode())
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
        item = inventory.entry(ref, part, item.normalizer_id)
    with pytest.raises(ValueError, match=rf"\A{re.escape(message)}\Z"):
        replay(item, document)


@pytest.mark.parametrize(
    "change",
    [
        "empty",
        "version",
        "revision",
        "code_path",
        "code_hash",
        "config_hash",
        "python",
        "unicode",
        "dependencies",
        "parser_provider",
        "mixed_revision",
    ],
)
def test_complete_recipe_pins_cannot_be_relaxed(
    template_case: Case, change: str
) -> None:
    original = template_case.pins
    modified = original
    if change == "empty":
        modified = ()
    else:
        item = original[0]
        changes: dict[str, JsonValue] = {
            "version": "unknown",
            "revision": "0" * 40,
            "code_path": "carddb/src/sve_carddb/html.py",
            "code_hash": digest(b"wrong"),
            "config_hash": digest(b"wrong"),
        }
        if change in changes:
            item = item.model_copy(
                update={
                    {"version": "id", "revision": "code_revision"}.get(
                        change, change
                    ): changes[change]
                }
            )
        elif change in {"python", "unicode", "dependencies"}:
            config = dict(item.config)
            config[
                {"python": "python_version", "unicode": "unicode_version"}.get(
                    change, change
                )
            ] = "wrong" if change != "dependencies" else {}
            item = item.model_copy(
                update={"config": config, "config_hash": digest(canonical(config))}
            )
        elif change == "parser_provider":
            config = {"provider": "en"}
            modified = (
                item,
                original[1].model_copy(
                    update={"config": config, "config_hash": digest(canonical(config))}
                ),
            )
        else:
            modified = (
                item,
                original[1].model_copy(update={"code_revision": "0" * 40}),
            )
        if change not in {"parser_provider", "mixed_revision"}:
            modified = (item, original[1])
    message = (
        "Immutable dependency batch contains a missing/non-blob object"
        if change == "revision"
        else "Template recipe pins or Python/Unicode runtime do not match"
    )
    with pytest.raises(ValueError, match=rf"\A{re.escape(message)}\Z"):
        pins.verify_recipes(template_case.repository, modified)


def test_committed_different_program_bytes_cannot_supply_the_loaded_recipe(
    template_case: Case, tmp_path: Path
) -> None:
    root = tmp_path / "repository"
    shutil.copytree(template_case.repository, root)
    (root / "carddb/src/sve_carddb/template_sources/normalizer.py").write_text(
        "# Synthetic different implementation\n"
    )
    revision = commit(root)
    with pytest.raises(
        ValueError, match=r"\AHistorical template runtime cannot be replayed\Z"
    ):
        pins.recipes(root, revision)


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
    template_case: Case, tmp_path: Path, change: str
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
    scan = scan_batch(
        FrozenSources(store.root, store.store_id, batch.batch_id),
        repository=template_case.repository,
        pins=template_case.pins,
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
        scan_batch(
            sources, repository=template_case.repository, pins=template_case.pins
        )


def test_inventory_yaml_is_bounded_deterministic_and_contains_no_source_prose(
    template_case: Case, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setattr(output, "CHUNK_ENTRIES", 3)
    reports = [
        command.run(arguments(template_case, tmp_path / name))
        for name in ("one", "two")
    ]
    assert reports[0] == reports[1]
    assert reports[0]["complete"] is True
    left = sorted((tmp_path / "one").rglob("*"))
    for path in left:
        if not path.is_file():
            continue
        raw = path.read_bytes()
        assert (
            raw == (tmp_path / "two" / path.relative_to(tmp_path / "one")).read_bytes()
        )
        assert not any(
            text.encode() in raw
            for text in ("Alpha", "Synthetic token", "Synthetic reminder", "Beta『X』N")
        )
        if path.suffix == ".yaml":
            assert len(raw) < MAX_BYTES
            wire = read_yaml(path)
            Inventory.model_validate_json(canonical(wire))
            assert (
                digest(canonical(wire))
                == object_value(reports[0]["inventories"])[
                    path.relative_to(tmp_path / "one").as_posix()
                ]
            )


@pytest.mark.parametrize(
    "location", ["store", "repository", "legacy", "exists", "symlink"]
)
def test_output_cannot_overwrite_immutable_inputs_or_existing_evidence(
    template_case: Case, tmp_path: Path, location: str
) -> None:
    destinations = {
        "store": template_case.store / "new",
        "repository": template_case.repository / "new",
        "legacy": template_case.legacy_path,
        "exists": tmp_path,
        "symlink": tmp_path / "link",
    }
    if location == "symlink":
        destinations[location].symlink_to(template_case.store, target_is_directory=True)
    with pytest.raises(
        ValueError,
        match=r"\ATemplate output must be new and outside all immutable inputs\Z",
    ):
        output.write(
            destinations[location],
            template_case.scan,
            {},
            inputs=(
                template_case.store,
                template_case.repository,
                template_case.legacy_path,
            ),
        )


def test_expected_legacy_count_is_explicit(template_case: Case, tmp_path: Path) -> None:
    args = arguments(template_case, tmp_path / "output")
    args.expected_templates = 3669
    with pytest.raises(
        ValueError,
        match=r"\ALegacy template count differs from the explicit checkpoint expectation\Z",
    ):
        command.run(args)


def test_comparison_file_hash_pins_the_bytes_actually_compared(
    template_case: Case, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    original = template_case.legacy_path.read_bytes()
    path = tmp_path / "legacy.jsonl"
    path.write_bytes(original)
    args = arguments(template_case, tmp_path / "output")
    args.legacy = path

    def scan_then_change(
        _sources: FrozenSources, *, repository: Path, pins: tuple[Recipe, ...]
    ) -> Scan:
        assert repository == template_case.repository
        assert pins == template_case.pins
        path.write_bytes(b"Synthetic changed comparison input")
        return template_case.scan

    monkeypatch.setattr(command, "scan_batch", scan_then_change)
    assert command.run(args)["legacy_file_hash"] == digest(original)


def test_oversized_recipe_envelope_fails_and_cleans_partial_output(
    template_case: Case, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setattr(output, "MAX_BYTES", 1)
    with pytest.raises(
        ValueError,
        match=r"\ATemplate inventory envelope exceeds the authored size limit\Z",
    ):
        output.write(
            tmp_path / "result",
            template_case.scan,
            {},
            inputs=(
                template_case.repository,
                template_case.store,
                template_case.legacy_path,
            ),
        )
    assert list(tmp_path.iterdir()) == []


def test_size_splitter_measures_each_final_recipe_envelope(
    template_case: Case, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setattr(output, "TARGET_BYTES", 1)
    chunks = list(
        output._chunks(template_case.pins, tuple(template_case.scan.entries[:4]))
    )
    assert len(chunks) == 4
    assert all(len(raw) < MAX_BYTES for raw, _ in chunks)


def test_runtime_modules_must_not_be_symlinks(
    template_case: Case, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    root = tmp_path / "runtime"
    modules = root / "carddb/src/sve_carddb"
    modules.mkdir(parents=True)
    (modules / "changed.py").symlink_to(
        template_case.repository / "carddb/src/sve_carddb/html.py"
    )
    monkeypatch.setattr(pins, "RUNTIME", root)
    with pytest.raises(
        ValueError, match=r"\ATemplate runtime cannot contain symlinked modules\Z"
    ):
        pins.recipes(template_case.repository, template_case.revision)


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


@pytest.mark.parametrize("failed", [False, True])
def test_cli_exit_and_redacted_output_are_stable_with_color_enabled(
    template_case: Case,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
    failed: bool,
) -> None:
    monkeypatch.setenv("FORCE_COLOR", "1")
    monkeypatch.setenv("TERM", "xterm-256color")
    monkeypatch.setattr(
        sys,
        "argv",
        [
            "template_sources",
            "--store",
            str(template_case.store),
            "--store-id",
            "test-store",
            "--batch-id",
            template_case.batch,
            "--repository",
            str(template_case.repository),
            "--code-revision",
            template_case.revision,
            "--legacy",
            str(template_case.legacy_path),
            "--expected-templates",
            "999" if failed else "3",
            "--output",
            str(tmp_path / "result"),
        ],
    )
    with pytest.raises(SystemExit) as result:
        command.main()
    assert result.value.code == (2 if failed else 0)
    captured = capsys.readouterr()
    assert captured.err == (
        "Template checkpoint failed; immutable inputs or recipe could not be verified\n"
        if failed
        else ""
    )
    assert captured.out == (
        ""
        if failed
        else '{"fingerprints":true,"legacy_member_coverage":true,"source_coverage":true}\n'
    )


@pytest.mark.parametrize(
    "failure", ["none", "source_coverage", "legacy_member_coverage", "fingerprints"]
)
def test_cli_persists_each_independent_failure_and_exits_one(
    template_case: Case,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
    failure: str,
) -> None:
    scan = copy.deepcopy(template_case.scan)
    if failure == "source_coverage":
        scan.failures.append({"reason": "unknown_effect_presence"})
    elif failure == "legacy_member_coverage":
        scan.occurrences = [
            item
            for item in scan.occurrences
            if item.member_hash != digest(b"SYN-02#1/text/0")
        ]
    elif failure == "fingerprints":
        scan.occurrences = [
            item
            for item in scan.occurrences
            if item.template != template_case.legacy[1].identifier
        ]

    def selected_scan(
        _sources: FrozenSources, *, repository: Path, pins: tuple[Recipe, ...]
    ) -> Scan:
        assert repository == template_case.repository
        assert pins == template_case.pins
        return scan

    args = arguments(template_case, tmp_path / "result")

    def selected_args(_parser: argparse.ArgumentParser) -> argparse.Namespace:
        return args

    monkeypatch.setattr(command, "scan_batch", selected_scan)
    monkeypatch.setattr(argparse.ArgumentParser, "parse_args", selected_args)
    with pytest.raises(SystemExit) as outcome:
        command.main()
    assert outcome.value.code == (0 if failure == "none" else 1)
    report = object_value(parse((args.output / "checkpoint.json").read_bytes()))
    assert report["complete"] is (failure == "none")
    statuses: dict[str, JsonValue] = {
        "fingerprints": failure != "fingerprints",
        "source_coverage": failure != "source_coverage",
        "legacy_member_coverage": failure
        not in {"fingerprints", "legacy_member_coverage"},
    }
    assert {key: object_value(report[key])["complete"] for key in statuses} == statuses
    captured = capsys.readouterr()
    assert not captured.err
    assert captured.out == canonical(statuses).decode() + "\n"


def test_jp_card_batch_with_nonpage_media_is_refused(
    template_case: Case, tmp_path: Path
) -> None:
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
        scan_batch(
            FrozenSources(store.root, store.store_id, batch.batch_id),
            repository=template_case.repository,
            pins=template_case.pins,
        )


def test_candidate_id_collision_is_refused_before_publication(
    template_case: Case, monkeypatch: pytest.MonkeyPatch
) -> None:
    original = inventory.entry

    def colliding_entry(ref: SourceRef, part: Part, normalizer_id: str) -> Entry:
        return original(ref, part, normalizer_id).model_copy(
            update={"id": "inv:synthetic-collision"}
        )

    monkeypatch.setattr(inventory, "entry", colliding_entry)
    with pytest.raises(
        ValueError, match=r"\ATemplate inventory entry IDs must be unique\Z"
    ):
        scan_batch(
            FrozenSources(template_case.store, "test-store", template_case.batch),
            repository=template_case.repository,
            pins=template_case.pins,
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
        scan_batch(
            FrozenSources(template_case.store, "test-store", template_case.batch),
            repository=template_case.repository,
            pins=template_case.pins,
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
