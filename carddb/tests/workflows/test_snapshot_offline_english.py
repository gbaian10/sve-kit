"""English display exceptions use the normal build, projection and snapshot readers."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.build.database import open_database
from sve_carddb.build.t1 import compile_build
from sve_carddb.contracts.four_layer import FaceRevisionOwner, OwnerField
from sve_carddb.core.json import array, canonical, digest, object_value, parse
from sve_carddb.core.provenance import InputRecord
from sve_carddb.domains.catalog import adoption_importer
from sve_carddb.domains.translations import sources as source_projection
from sve_carddb.domains.translations.four_layer_authored import from_files
from sve_carddb.domains.translations.four_layer_sources import descriptor
from sve_carddb.domains.translations.inputs import Snapshot, load_glossary
from sve_carddb.domains.translations.sources import Sources
from sve_carddb.export.media import prepare_media
from sve_carddb.export.reader import read_snapshot, read_text_all
from sve_carddb.export.transport import export_snapshot
from sve_carddb.workflows.offline import build

from .test_snapshot_offline import prepared as prepared  # ruff: ignore[useless-import-alias] -- reuse frozen regional fixture
from .test_snapshot_offline_templates import RULE_CONTENT, SCHEMA
from .test_snapshot_offline_templates import default_text_case as default_text_case  # ruff: ignore[useless-import-alias] -- reuse complete JP and EN source coverage

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue

    from sve_carddb.contracts.source_binding import SourceDescriptor
    from sve_carddb.domains.card_extras import CardPage
    from sve_carddb.workflows.offline import Inputs

    from ..support.text_observation_fixtures import Case


def records(source: SourceDescriptor, card: str, face: str) -> list[JsonValue]:
    return [
        {
            "kind": "english_exception_target",
            "origin": "machine",
            "low_confidence": True,
            "data": {
                "id": "synthetic.english.rule",
                "source_hash": source.source_hash,
                "lang": "zh-Hant",
                "references": {},
                "nodes": [{"kind": "Literal", "text": "合成規則😀。"}],
            },
        },
        {
            "kind": "english_exception_use",
            "data": {
                "source": source.model_dump(mode="json"),
                "card_id": card,
                "face_id": face,
                "target_id": "synthetic.english.rule",
                "identity": "confirmed_no_jp",
                "reason": "Synthetic current identity review",
            },
        },
    ]


def install_inputs(root: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    (root / "translations/glossary").mkdir(parents=True)
    rules = root / "translations/parameter-rules/current.yaml"
    rules.parent.mkdir(parents=True)
    rules.write_bytes(RULE_CONTENT)

    class CurrentInputs:
        def load(self) -> Snapshot:
            return load_glossary(root)

    class AdoptedInputs:
        def translation_inputs(self) -> CurrentInputs:
            return CurrentInputs()

        def configuration(self) -> dict[str, JsonValue]:
            return {
                "synthetic_adoptions": "immutable-receipts",
                "translations": {
                    path: digest(raw) for path, raw, _ in load_glossary(root).closure
                },
            }

    monkeypatch.setattr(
        adoption_importer, "AdoptionInputs", lambda *_args, **_kwargs: AdoptedInputs()
    )

    def synthetic_project(
        raw: bytes, _url: str, provider: str
    ) -> tuple[str, JsonValue]:
        # This shared fixture seals synthetic JSON envelopes instead of official HTML.
        assert raw.startswith(b"<html>")
        assert raw.endswith(b"</html>")
        return ("ja" if provider == "jp" else "en"), parse(raw[6:-7])

    monkeypatch.setattr(source_projection, "project", synthetic_project)


@pytest.mark.parametrize(
    "mode",
    [
        "translated",
        "missing",
        "unresolved",
        "stale",
        "wrong_face",
        "wrong_locator",
        "wrong_version",
    ],
)
def test_normal_build_selects_only_exact_confirmed_english_fields(  # ruff: ignore[too-many-locals,too-many-statements] -- independent builds verify selection and public round-trip together
    prepared: tuple[Case, Inputs, tuple[CardPage, ...]],
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    mode: str,
) -> None:
    _, recipe, _ = prepared
    root = recipe.repo / "authored"
    install_inputs(root, monkeypatch)
    before = build(recipe, bundle_dir=tmp_path / "before")
    printing = next(
        row
        for row in before.projection.tables["printing"]
        if row["card_no"] == "GF01-001EN"
    )
    face = next(
        row
        for row in before.projection.tables["face"]
        if row["card_id"] == printing["card_id"]
    )
    revision = next(
        row
        for row in before.projection.tables["face_revision"]
        if row["face_id"] == face["id"] and row["region"] == "en"
    )
    context = InputRecord.model_validate_json(before.input_content).context
    sources = Sources({recipe.store_id: recipe.archive}, recipe.repo, context)
    with open_database(compile_build(SCHEMA), tmp_path / "before/build.sqlite") as db:
        source = descriptor(
            db,
            sources,
            OwnerField(
                owner=FaceRevisionOwner(
                    kind="face_revision", revision_id=str(revision["id"])
                ),
                field="effect",
                ordinal=None,
            ),
            recipe.sources[0].card_batch,
            lang="en",
        )
    values = records(source, str(printing["card_id"]), str(face["id"]))
    use = object_value(object_value(values[1])["data"])
    selected_source = object_value(use["source"])
    ref = object_value(selected_source["source_ref"])
    if mode == "unresolved":
        use["identity"] = "unresolved"
    elif mode == "stale":
        selected_source["source_hash"] = ref["text_hash"] = "f" * 64
        object_value(object_value(values[0])["data"])["source_hash"] = "f" * 64
    elif mode == "wrong_face":
        use["face_id"] = "another-face"
    elif mode == "wrong_locator":
        ref["locator"] = "/faces/1/text"
    elif mode == "wrong_version":
        ref["source_version_id"] = "src:v1:" + "f" * 64
    if mode != "missing":
        path = root / "translations/overrides/english/000.yaml"
        path.parent.mkdir(parents=True)
        path.write_bytes(
            canonical({"format": 3, "kind": "translation_shard", "records": values})
        )
    after = build(recipe, bundle_dir=tmp_path / "after")
    projected = next(
        row
        for row in after.projection.tables["face_revision"]
        if row["id"] == revision["id"]
    )
    translations = [
        object_value(v)
        for v in array(projected["translations"])
        if object_value(v)["field"] == "effect"
    ]
    translated = mode == "translated"
    assert len(translations) == int(translated)
    unit = next(
        row
        for row in after.projection.tables["text_unit"]
        if row["id"] == projected["effect_unit_id"]
    )
    assert unit["text"] == "Rule."
    if translated:
        assert translations[0]["basis"] == "own_source"
        translation = next(
            row
            for row in after.projection.tables["translation"]
            if row["id"] == translations[0]["translation_id"]
        )
        text = next(
            row
            for row in after.projection.tables["text_unit"]
            if row["id"] == translation["text_unit_id"]
        )
        assert (
            text["text"],
            translation["origin"],
            translation["authority"],
            translation["low_confidence"],
        ) == ("合成規則😀。", "machine", "unofficial", True)
        assert translation["annotation_set_id"] is None
        english = object_value(
            object_value(after.report["effect_translations"])["english_exceptions"]
        )
        assert english["translated"] == english["low_confidence"] == 1
    elif mode != "missing":
        english = object_value(
            object_value(after.report["effect_translations"])["english_exceptions"]
        )
        assert english["translated"] == 0
        assert english["original"] == 1
        reasons = object_value(english["fallback_reasons"])
        expected = (
            "unresolved_english_identity"
            if mode == "unresolved"
            else "english_owner_face_mismatch"
            if mode == "wrong_face"
            else "english_source_mismatch"
        )
        assert reasons == {expected: 1}
    with open_database(compile_build(SCHEMA), tmp_path / "after/build.sqlite") as db:
        assert db.rows("text_template_binding") == ()
        assert db.rows("translation_use_annotation") == ()
    assert (
        after.projection.tables["card_engine_support"]
        == before.projection.tables["card_engine_support"]
    )
    if mode == "missing":
        assert before.projection.tables == after.projection.tables
    snapshot = export_snapshot(
        prepare_media(after.projection, None, revision=1).projection,
        after.ownership,
        recipe.batch(),
    )
    payloads = {key: blob.raw for key, blob in snapshot.payloads.items()}
    assert read_snapshot(snapshot.manifest, payloads) == after.projection.tables
    assert (
        read_text_all(
            snapshot.manifest,
            snapshot.text_all.raw,
            {
                key: value
                for key, value in payloads.items()
                if key == "programs" or key.startswith("images/")
            },
        )
        == after.projection.tables
    )


def test_english_reader_rejects_missing_targets_and_omitted_required_references() -> (
    None
):
    target: dict[str, JsonValue] = {
        "kind": "english_exception_target",
        "data": {
            "id": "synthetic.english",
            "source_hash": "a" * 64,
            "lang": "zh-Hant",
            "references": {"term": {"kind": "glossary", "key": "term:synthetic"}},
            "nodes": [{"kind": "Literal", "text": "Synthetic target."}],
        },
    }

    def load() -> None:
        raw = canonical({"format": 3, "kind": "translation_shard", "records": [target]})
        from_files((("translations/overrides/english/000.yaml", raw, raw),))

    with pytest.raises(ValueError, match="Invalid four-layer authored shard"):
        load()
    object_value(target["data"])["nodes"] = [{"kind": "LeafRef", "slot": "term"}]
    with pytest.raises(ValueError, match="requires a glossary concept"):
        load()
