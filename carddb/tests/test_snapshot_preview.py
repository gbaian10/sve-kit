"""Synthetic preview isolation, public identity and unknown-coverage counterexamples."""

from dataclasses import replace
from typing import TYPE_CHECKING
from zlib import compress

import pytest
from pydantic import JsonValue
from typer.testing import CliRunner

import sve_carddb.snapshot.preview as writer_module
import sve_carddb.snapshot.preview.build as build_module
import sve_carddb.snapshot.preview.commands as cli_module
from sve_carddb.build_db import create_database
from sve_carddb.cli import app
from sve_carddb.products import load_products
from sve_carddb.products.identities import ProductIdentities
from sve_carddb.products.plan import OfficialProducts
from sve_carddb.registry.records import PrintingData
from sve_carddb.registry.snapshot import load_registry
from sve_carddb.snapshot.export import Brotli, Ownership, export_snapshot
from sve_carddb.snapshot.preview import (
    Roots,
    _write,
    require_unknown_coverage,
    write_preview,
)
from sve_carddb.snapshot.preview.build import (
    Built,
    Inputs,
    build,
    exclusions,
    publication_printings,
)
from sve_carddb.snapshot.preview.commands import command_brotli, verify_inputs
from sve_carddb.snapshot.project import project
from sve_carddb.snapshot.project.records import art_records, initial
from sve_carddb.snapshot.project.source import Source
from sve_carddb.snapshot.publication import require_formal, require_preview
from sve_carddb.snapshot.reader import read_snapshot
from sve_carddb.snapshot.values import (
    array,
    canonical,
    digest,
    object_value,
    parse,
    string,
)
from sve_carddb.text_observations import plan_text_observations

from .registry_snapshot_fixtures import edit_record
from .snapshot_project_fixtures import SETTINGS, populate, schema
from .test_snapshot_export import BATCH
from .test_snapshot_export import exported as exported  # ruff: ignore[useless-import-alias] -- register shared module fixture
from .test_snapshot_project import projected
from .text_observation_fixtures import LANGUAGES

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.registry.storage import Entry
    from sve_carddb.snapshot.export import Snapshot
    from sve_carddb.snapshot.project import Projection

    from .shared_case_fixtures import CorrectionCaseTemplate, TextCaseTemplate
    from .text_observation_fixtures import Case


@pytest.fixture(scope="module")
def logical() -> tuple[Projection, Ownership]:
    with create_database(schema()) as db:
        with db.transaction():
            populate(db)
        projection = projected(db)
        projection = replace(
            projection,
            tables=projection.tables
            | {
                "image_asset": [
                    row | {"availability": "unfetched", "publication_state": "pending"}
                    for row in projection.tables["image_asset"]
                ],
                "image_variant": [],
            },
        )
        return projection, Ownership.from_database(db, projection)


@pytest.fixture(params=["formal_version", "en_region"])
def invalid_preview(request: pytest.FixtureRequest, exported: Snapshot) -> Snapshot:
    change: dict[str, JsonValue] = (
        {"data_version": "20261002T010203Z-0001"}
        if request.param == "formal_version"
        else {"regions": ["jp", "en"]}
    )
    return replace(exported, manifest=exported.manifest | change)


def test_writer_rejects_non_preview_manifest(
    invalid_preview: Snapshot, tmp_path: Path
) -> None:
    with pytest.raises(ValueError, match="Preview requires"):
        write_preview(
            invalid_preview, Roots(tmp_path / "preview", tmp_path / "formal"), {}
        )
    assert list(tmp_path.iterdir()) == []


def test_export_entry_rejects_non_preview_manifest(
    invalid_preview: Snapshot,
    logical: tuple[Projection, Ownership],
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    path = tmp_path / "inputs.json"
    path.write_bytes(canonical(cli_recipe(tmp_path)))
    monkeypatch.setattr(
        cli_module, "build", lambda _recipe: Built(logical[0], logical[1], b"input", {})
    )
    monkeypatch.setattr(
        cli_module, "export_snapshot", lambda *_args, **_kw: invalid_preview
    )
    # Isolate the command's guard from the writer's independent guard.
    monkeypatch.setattr(cli_module, "write_preview", lambda *_args, **_kw: {})
    result = CliRunner().invoke(
        app,
        ["snapshot", "export", "--inputs", str(path)],
        env={
            "SVE_PREVIEW_DIR": str(tmp_path / "preview"),
            "SVE_CDN_DIR": str(tmp_path / "formal"),
        },
    )
    assert isinstance(result.exception, ValueError)
    assert str(result.exception).startswith("Preview requires")
    assert not (tmp_path / "preview").exists()
    assert not (tmp_path / "formal").exists()


@pytest.mark.parametrize("version", ["20261002T010203Z-0001", "20261002T010203Z-0002"])
def test_preview_cannot_accept_formal_version(exported: Snapshot, version: str) -> None:
    manifest = exported.manifest | {"data_version": version}
    with pytest.raises(ValueError, match="preview- data version"):
        require_preview(manifest)


def test_formal_publish_refuses_preview(exported: Snapshot, tmp_path: Path) -> None:
    with pytest.raises(ValueError, match="Formal publish refuses preview"):
        require_formal(exported.manifest)
    path = tmp_path / "manifest.json"
    path.write_bytes(canonical(exported.manifest))
    result = CliRunner().invoke(app, ["snapshot", "publish", str(path)])
    assert result.exit_code != 0
    assert isinstance(result.exception, ValueError)
    assert str(result.exception) == "Formal publish refuses preview artifacts"
    require_formal(exported.manifest | {"data_version": "20261002T010203Z-0001"})


def test_preview_scope_is_exactly_jp(exported: Snapshot) -> None:
    for regions in (["en"], ["jp", "en"]):
        broken = exported.manifest.copy()
        broken["regions"] = list[JsonValue](regions)
        with pytest.raises(ValueError, match="exactly the JP"):
            require_preview(broken)


@pytest.mark.parametrize("relation", ["equal", "preview_child", "formal_child"])
def test_overlapping_roots_are_refused(tmp_path: Path, relation: str) -> None:
    preview, formal = tmp_path / "root", tmp_path / "root"
    if relation == "preview_child":
        preview /= "child"
    if relation == "formal_child":
        formal /= "child"
    with pytest.raises(ValueError, match="disjoint"):
        Roots(preview, formal).verify()
    assert not preview.exists()
    assert not formal.exists()


@pytest.mark.parametrize("relation", ["equal", "preview_child", "formal_child"])
def test_direct_writer_refuses_overlapping_roots(
    exported: Snapshot, tmp_path: Path, relation: str
) -> None:
    preview, formal = tmp_path / "root", tmp_path / "root"
    if relation == "preview_child":
        preview /= "child"
    if relation == "formal_child":
        formal /= "child"
    with pytest.raises(ValueError, match="disjoint"):
        write_preview(exported, Roots(preview, formal), {})
    assert list(tmp_path.iterdir()) == []


def test_symlink_roots_and_destinations_cannot_touch_formal(tmp_path: Path) -> None:
    formal = tmp_path / "formal"
    formal.mkdir()
    alias = tmp_path / "alias"
    alias.symlink_to(formal, target_is_directory=True)
    with pytest.raises(ValueError, match="disjoint"):
        Roots(alias / "nested", formal).verify()
    preview = tmp_path / "preview"
    preview.mkdir()
    (preview / "snapshots").symlink_to(formal, target_is_directory=True)
    roots = Roots(preview, formal)
    with pytest.raises(ValueError, match="escapes"):
        _write(roots, "snapshots/preview/current.json", b"{}", immutable=False)
    with pytest.raises(ValueError, match="escapes"):
        roots.destination("../formal/index.json")
    assert list(formal.iterdir()) == []


def test_writer_preserves_formal_state_and_independent_join(
    exported: Snapshot, logical: tuple[Projection, Ownership], tmp_path: Path
) -> None:
    roots = Roots(tmp_path / "preview", tmp_path / "formal")
    roots.formal.mkdir()
    sentinel = roots.formal / "active-cache"
    sentinel.write_bytes(b"formal state")
    report = write_preview(exported, roots, {"input_sha256": "sha256:" + "a" * 64})
    pointer = object_value(
        parse((roots.preview / "snapshots/preview/current.json").read_bytes())
    )
    manifest_raw = (roots.preview / string(pointer["manifest_path"])).read_bytes()
    assert digest(manifest_raw) == pointer["manifest_sha256"]
    assert report["pointer"] == pointer
    assert (
        read_snapshot(
            parse(manifest_raw),
            {key: blob.raw for key, blob in exported.payloads.items()},
        )
        == logical[0].tables
    )
    assert not (roots.preview / "snapshots/versions").exists()
    assert not (roots.preview / "pages").exists()
    assert sentinel.read_bytes() == b"formal state"
    assert list(roots.formal.iterdir()) == [sentinel]
    assert (
        write_preview(exported, roots, {"input_sha256": "sha256:" + "a" * 64}) == report
    )


def test_failed_artifact_write_keeps_old_preview_pointer(
    exported: Snapshot, tmp_path: Path
) -> None:
    roots = Roots(tmp_path / "preview", tmp_path / "formal")
    _write(roots, "snapshots/preview/current.json", b"old preview", immutable=False)
    first = object_value(array(exported.manifest["files"])[0])
    _write(roots, string(first["path"]), b"corrupt existing bytes", immutable=True)
    with pytest.raises(ValueError, match="Immutable"):
        write_preview(exported, roots, {})
    assert (
        roots.preview / "snapshots/preview/current.json"
    ).read_bytes() == b"old preview"


@pytest.mark.parametrize("with_brotli", [False, True])
def test_preview_pointer_follows_every_immutable_member(
    exported: Snapshot,
    logical: tuple[Projection, Ownership],
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    with_brotli: bool,
) -> None:
    codec = Brotli("synthetic-test-codec-v1", compress) if with_brotli else None
    if codec is not None:
        exported = export_snapshot(logical[0], logical[1], BATCH, brotli=codec)
    roots = Roots(tmp_path / "preview", tmp_path / "formal")
    manifest_hash = digest(canonical(exported.manifest))[7:]
    raw_paths = {
        string(object_value(item)["path"]) for item in array(exported.manifest["files"])
    } | {
        string(object_value(exported.manifest["text_all"])["path"]),
        "snapshots/manifests/" + manifest_hash + ".json",
    }
    expected = raw_paths | {path + ".gz" for path in raw_paths}
    if codec is not None:
        expected |= {path + ".br" for path in raw_paths}
    expected.add("reports/" + manifest_hash + ".json")
    sealed: set[str] = set()
    pointer_written = False

    def observed_write(roots: Roots, path: str, raw: bytes, *, immutable: bool) -> None:
        nonlocal pointer_written
        if not immutable:
            assert path == "snapshots/preview/current.json"
            assert expected <= sealed
            assert all((roots.preview / member).is_file() for member in expected)
            pointer_written = True
        else:
            assert not pointer_written
        _write(roots, path, raw, immutable=immutable)
        if immutable:
            sealed.add(path)

    monkeypatch.setattr(writer_module, "_write", observed_write)
    write_preview(exported, roots, {}, brotli=codec)
    assert pointer_written


@pytest.mark.parametrize("late_member", ["snapshots/manifests/", "reports/"])
def test_late_immutable_failure_preserves_old_pointer(
    exported: Snapshot,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    late_member: str,
) -> None:
    roots = Roots(tmp_path / "preview", tmp_path / "formal")
    _write(roots, "snapshots/preview/current.json", b"old preview", immutable=False)

    def failing_write(roots: Roots, path: str, raw: bytes, *, immutable: bool) -> None:
        if path.startswith(late_member):
            raise OSError("Synthetic late immutable failure")
        _write(roots, path, raw, immutable=immutable)

    monkeypatch.setattr(writer_module, "_write", failing_write)
    with pytest.raises(OSError, match="late immutable"):
        write_preview(exported, roots, {})
    assert (
        roots.preview / "snapshots/preview/current.json"
    ).read_bytes() == b"old preview"


@pytest.mark.parametrize("field", ["source_windows", "restriction_coverage"])
def test_missing_coverage_cannot_be_invented_complete(
    logical: tuple[Projection, Ownership], field: str
) -> None:
    projection = replace(
        logical[0],
        metadata=logical[0].metadata
        | {"source_windows": [], "restriction_coverage": []},
        tables=logical[0].tables
        | {name: [] for name in ("qa", "errata", "cr_version", "restriction")},
    )
    require_unknown_coverage(projection)
    broken = replace(
        projection, metadata=projection.metadata | {field: [{"state": "complete"}]}
    )
    with pytest.raises(ValueError, match="Uncovered"):
        require_unknown_coverage(broken)
    for table in ("qa", "errata", "cr_version", "restriction"):
        with pytest.raises(ValueError, match="Unrequested"):
            require_unknown_coverage(
                replace(
                    projection, tables=projection.tables | {table: [{"id": "invented"}]}
                )
            )


def test_roots_have_no_defaults() -> None:
    result = CliRunner().invoke(
        app,
        ["snapshot", "export", "--inputs", __file__],
        env={
            "SVE_PREVIEW_DIR": "",
            "SVE_CDN_DIR": "",
            "NO_COLOR": "1",
            "TERM": "dumb",
        },
    )
    assert result.exit_code != 0
    assert "--preview-dir" in result.output


@pytest.mark.parametrize("publish_printings", [True, False])
def test_build_keeps_errata_pending_and_filters_diagnostic_identity(
    default_text_case: TextCaseTemplate,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    publish_printings: bool,
) -> None:
    case = default_text_case.copy(tmp_path / "synthetic")
    cards = case.provider.cards.copy()
    for key, card in cards.items():
        if key[0] == "jp":
            cards[key] = card.model_copy(update={"has_errata_link": True})
    case.provider.cards = cards
    case.plan = plan_text_observations(case.identity, case.provider)
    assert case.plan.diagnostic_exclusions != case.plan.publication_identity()
    pub = publication_printings(case.plan.publication_identity())
    assert pub
    assert exclusions(case.plan.publication_identity()) == []
    recipe = prepare_build(case, tmp_path, monkeypatch)
    if not publish_printings:
        pub = frozenset()
        monkeypatch.setattr(build_module, "publication_printings", lambda _plan: pub)
    built = build(recipe)
    assert {row["id"] for row in built.projection.tables["printing"]} == pub
    assert built.report["errata_link_printings"] == len(pub)
    if not publish_printings:
        assert built.projection.tables["card"] == []
        assert built.projection.tables["face"] == []
        assert built.projection.tables["face_revision"] == []
    assert bool(built.report["pending_face_regions"]) == publish_printings
    assert built.report["excluded_printings"] == []
    assert built.projection.metadata["source_windows"] == []
    assert built.projection.metadata["restriction_coverage"] == []
    assert built.report["input_sha256"] == digest(built.input_content)
    snapshot = export_snapshot(built.projection, built.ownership, recipe.batch())
    snapshot.verify(built.projection)
    with pytest.raises(ValueError, match="immutable input"):
        verify_inputs(Roots(recipe.repo / "output", tmp_path / "formal"), recipe)


def prepare_build(
    case: Case,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    *,
    revision: str = "a" * 40,
) -> Inputs:
    repo = tmp_path / "repo"
    (repo / "carddb/src/sve_carddb").mkdir(parents=True, exist_ok=True)
    (repo / "carddb/uv.lock").write_bytes(b"synthetic lock")
    vocabulary = tmp_path / "vocabulary.json"
    vocabulary.write_bytes(canonical(case.vocabulary.model_dump(mode="json")))
    recipe = Inputs(
        repo=repo,
        archive=case.store,
        store_id="test-store",
        card_batch="sha256:" + "a" * 64,
        image_batch="sha256:" + "b" * 64,
        revision=revision,
        parser_version="synthetic-v1",
        vocabulary=vocabulary,
        languages=LANGUAGES,
        as_of="2026-10-02",
        data_version=BATCH.data_version,
        published_at=BATCH.published_at,
        feedback_url="https://example.invalid/feedback",
        grammar_version="synthetic-v1",
        normalizer_version="synthetic-v1",
    )
    monkeypatch.setattr(
        build_module, "plan_preview", lambda *_args, **_kwargs: case.identity
    )
    monkeypatch.setattr(
        build_module, "plan_text_observations", lambda *_args, **_kwargs: case.plan
    )
    monkeypatch.setattr(
        build_module, "load_products", lambda *_args, **_kwargs: case.catalog
    )

    identities = ProductIdentities(
        recipe.revision, digest(b"{}"), b"{}", (), {}, {}, (), case.catalog
    )
    monkeypatch.setattr(
        build_module, "load_product_identities", lambda *_args, **_kwargs: identities
    )
    for name in ("FrozenJP", "FrozenTexts", "FrozenImages", "FrozenProducts"):
        monkeypatch.setattr(
            build_module, name, lambda *_args, **_kwargs: EmptySources()
        )
    monkeypatch.setattr(
        build_module,
        "plan_official_products",
        lambda *_args, **_kwargs: OfficialProducts(
            identities, (), (), (), (), case.identity
        ),
    )
    return recipe


def test_build_rejects_invented_source_coverage(
    default_text_case: TextCaseTemplate,
    logical: tuple[Projection, Ownership],
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    recipe = prepare_build(
        default_text_case.copy(tmp_path / "synthetic"), tmp_path, monkeypatch
    )
    poisoned = replace(
        logical[0],
        metadata=logical[0].metadata | {"source_windows": [{"state": "complete"}]},
    )
    monkeypatch.setattr(build_module, "project", lambda *_args, **_kwargs: poisoned)
    monkeypatch.setattr(Ownership, "from_database", lambda _db, _projection: logical[1])
    with pytest.raises(ValueError, match="Uncovered sources must remain empty windows"):
        build(recipe)


def test_build_excludes_conflicted_publication_but_retains_identity(
    default_correction_case: CorrectionCaseTemplate,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    fixture = default_correction_case.copy(tmp_path / "synthetic")
    case = fixture.texts

    def conflict(entry: Entry) -> None:
        entry.data["expected_raw_value"] = "Wrong synthetic old value"

    edit_record(case.root, "source_correction", conflict)
    case.identity = replace(case.identity, snapshot=load_registry(case.root))
    case.catalog = load_products(case.root, registry=case.identity.snapshot)
    case.plan = plan_text_observations(
        case.identity, case.provider, images=fixture.images
    )
    assert case.plan.corrections is not None
    assert case.plan.corrections[0].status == "conflict"
    diagnostic = publication_printings(case.identity)
    publication = publication_printings(case.plan.publication_identity())
    withheld = diagnostic - publication
    assert withheld
    recipe = prepare_build(case, tmp_path, monkeypatch)
    built = build(recipe)
    public = {row["id"] for row in built.projection.tables["printing"]}
    assert public == publication
    assert not public & withheld
    assert built.report["excluded_printings"] == exclusions(
        case.plan.publication_identity()
    )
    assert built.report["excluded_printings"]
    export_snapshot(built.projection, built.ownership, recipe.batch()).verify(
        built.projection
    )


def test_project_refuses_publication_printing_absent_from_build() -> None:
    with create_database(schema()) as db:
        with db.transaction():
            populate(db)
        with pytest.raises(ValueError, match="Publication printing is absent"):
            project(
                db,
                regions=("jp",),
                as_of="2026-10-02",
                settings=SETTINGS,
                publication_printings=frozenset({"missing-synthetic-printing"}),
            )


class EmptySources:
    def pages(self) -> tuple[()]:
        return ()


def test_explicit_compressor_is_version_and_binary_pinned(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    executable = tmp_path / "compressor"
    executable.write_bytes(b"synthetic executable")
    calls: list[tuple[list[str], bytes | None]] = []

    def output(args: list[str], **kwargs: object) -> str | bytes:
        raw = kwargs.get("input")
        assert raw is None or isinstance(raw, bytes)
        calls.append((args, raw))
        return (
            "synthetic-compressor-v1\n"
            if kwargs.get("text")
            else b"synthetic compressed"
        )

    monkeypatch.setattr(
        "sve_carddb.snapshot.preview.commands.subprocess.check_output", output
    )
    codec = command_brotli(executable)
    assert codec.version == "synthetic-compressor-v1 / " + digest(
        executable.read_bytes()
    )
    assert codec.compress(b"synthetic bytes") == b"synthetic compressed"
    assert calls == [
        ([str(executable), "--version"], None),
        ([str(executable), "-q", "11", "-c"], b"synthetic bytes"),
    ]


def cli_recipe(tmp_path: Path) -> dict[str, JsonValue]:
    return {
        "repo": str(tmp_path / "repo"),
        "archive": str(tmp_path / "archive"),
        "store_id": "synthetic",
        "card_batch": "sha256:" + "a" * 64,
        "image_batch": "sha256:" + "b" * 64,
        "revision": "a" * 40,
        "parser_version": "synthetic",
        "vocabulary": str(tmp_path / "vocab"),
        "languages": [],
        "as_of": "2026-10-02",
        "data_version": BATCH.data_version,
        "published_at": BATCH.published_at,
        "feedback_url": "https://example.invalid/feedback",
        "grammar_version": "synthetic-v1",
        "normalizer_version": "synthetic-v1",
    }


def test_cli_export_explicit_env_roots(
    logical: tuple[Projection, Ownership],
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    path = tmp_path / "inputs.json"
    path.write_bytes(canonical(cli_recipe(tmp_path)))
    monkeypatch.setattr(
        cli_module,
        "build",
        lambda _recipe: Built(logical[0], logical[1], b"synthetic input", {}),
    )
    result = CliRunner().invoke(
        app,
        ["snapshot", "export", "--inputs", str(path)],
        env={
            "SVE_PREVIEW_DIR": str(tmp_path / "preview"),
            "SVE_CDN_DIR": str(tmp_path / "formal"),
        },
    )
    assert result.exit_code == 0, result.exception
    assert (tmp_path / "preview/snapshots/preview/current.json").exists()
    assert (
        tmp_path / "preview/private/inputs" / (digest(b"synthetic input")[7:] + ".json")
    ).read_bytes() == b"synthetic input"
    assert not (tmp_path / "formal").exists()


def test_review_joins_follow_filtered_primary_keys() -> None:
    with create_database(schema()) as db:
        with db.transaction():
            populate(db)
            db.insert(
                "decision",
                dict(db.rows("decision")[0].values)
                | {"id": "pending", "state": "proposed"},
            )
            db.insert(
                "digital_link",
                dict(db.rows("digital_link")[0].values)
                | {"id": "a-excluded", "decision_id": "pending"},
            )
        source = Source(db)
        view = initial(source)
        view["digital_link"] = [
            row for row in view["digital_link"] if row["id"] == "digital-link"
        ]
        art_records(source, view)
        assert view["digital_link"][0]["review_level"] == "confirmed"
        assert view["art"][0]["review_level"] == "confirmed"
        assert view["digital_art_link"][0]["review_level"] == "confirmed"


def test_exclusion_report_contains_only_genuine_jp_reasons(
    default_text_case: TextCaseTemplate,
) -> None:
    plan = default_text_case.plan.publication_identity()
    key = next(
        record.record_key
        for record in plan.included("printing")
        if isinstance(record.data, PrintingData) and record.data.region == "jp"
    )
    en_key = next(
        record.record_key
        for record in plan.included("printing")
        if isinstance(record.data, PrintingData) and record.data.region == "en"
    )
    changed = replace(
        plan,
        projections=tuple(
            replace(
                item,
                disposition="excluded",
                reasons=("synthetic_identity_not_adopted",),
            )
            if item.record_key in {key, en_key}
            else item
            for item in plan.projections
        ),
    )
    result = exclusions(changed)
    assert len(result) == 1
    data = plan.snapshot.records[key].data
    assert isinstance(data, PrintingData)
    assert result[0] == {
        "card_no": data.card_no,
        "reasons": ["synthetic_identity_not_adopted"],
    }
    assert len(publication_printings(changed)) == len(publication_printings(plan)) - 1


def test_renamed_preview_is_not_a_formal_release(
    exported: Snapshot, tmp_path: Path
) -> None:
    path = tmp_path / "renamed.json"
    path.write_bytes(
        canonical(exported.manifest | {"data_version": "20261002T010203Z-0001"})
    )
    result = CliRunner().invoke(app, ["snapshot", "publish", str(path)])
    assert result.exit_code != 0
    assert "Formal release gates are not implemented yet (#34)" in result.output
    assert sorted(p.name for p in tmp_path.iterdir()) == ["renamed.json"]
