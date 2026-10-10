"""Normal authored inputs, exact frozen owners and DB projection share one four-layer build."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.build.database import open_database
from sve_carddb.build.t1 import MINIMUM_CAPABILITIES, compile_build
from sve_carddb.contracts.four_layer import FaceRevisionOwner, OwnerField
from sve_carddb.core.json import array, canonical, digest, object_value, parse
from sve_carddb.core.provenance import InputRecord
from sve_carddb.domains.catalog import adoption_importer
from sve_carddb.domains.translations import sources as source_projection
from sve_carddb.domains.translations.four_layer_authored import (
    FrameRecord,
    TargetRecord,
)
from sve_carddb.domains.translations.four_layer_normalizer import normalize_source
from sve_carddb.domains.translations.four_layer_pipeline import prepare_classifier
from sve_carddb.domains.translations.four_layer_sources import (
    card_source,
    descriptor,
    semantic_context,
)
from sve_carddb.domains.translations.four_layer_storage import read_binding
from sve_carddb.domains.translations.inputs import Snapshot, load_glossary
from sve_carddb.domains.translations.recognition.rules import parse as parse_rules
from sve_carddb.domains.translations.sources import Sources
from sve_carddb.workflows.offline import build

from ..domains.registry.test_registry import card, make_inputs
from ..support.shared_case_fixtures import TextCaseTemplate
from ..support.text_observation_fixtures import make_case
from .test_snapshot_offline import prepared as prepared  # ruff: ignore[useless-import-alias] -- retain the actual frozen regional composition fixture

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue

    from sve_carddb.domains.card_extras import CardPage
    from sve_carddb.workflows.offline import Inputs

    from ..support.text_observation_fixtures import Case

SCHEMA = (
    *MINIMUM_CAPABILITIES,
    "en",
    "translation_evidence",
    "translation_names",
    "translation_templates",
    "rulings",
)
RAW = "カードを2枚引く。"
TEXTS = {
    "BP02-071": ("数値仮名", RAW),
    "PR-001": ("名前", "『名前』を1枚手札に加える。"),
    "BP02-072": ("別名", "『別名』を1枚手札に加える。"),
    "BP02-073": ("第三", "Rule."),
}
RULE_CONTENT = canonical(
    {
        "format": 2,
        "kind": "template_parameter_rules",
        "rules": [
            {
                "rule_id": "suffix_unit_cards",
                "enabled": True,
                "origin": "project",
                "low_confidence": False,
            }
        ],
    }
)


@pytest.fixture(scope="module")
def default_text_case(tmp_path_factory: pytest.TempPathFactory) -> TextCaseTemplate:
    """Each synthetic owner has its own page in the same batch the offline build selects."""
    inputs = make_inputs()
    for number, (name, text) in TEXTS.items():
        inputs.jp[number] = card(number, name, text=text)
    base = tmp_path_factory.mktemp("four-layer-text")
    return TextCaseTemplate.capture(make_case(base / "authored", inputs), base)


@pytest.mark.parametrize("translated", [True, False])
def test_normal_offline_build_reads_authored_frames_and_keeps_whole_field_fallback(  # ruff: ignore[too-many-locals,too-many-statements] -- independent before/after builds prove authored ingestion, frozen ownership and final field projection
    prepared: tuple[Case, Inputs, tuple[CardPage, ...]],
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    translated: bool,
) -> None:
    case, recipe, _ = prepared
    root = recipe.repo / "authored"
    (root / "translations/glossary").mkdir(parents=True)
    (root / "translations/parameter-rules").mkdir(parents=True)
    (root / "translations/parameter-rules/current.yaml").write_bytes(RULE_CONTENT)

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
        # The shared fixture archives a synthetic JSON envelope, never an official HTML page.
        assert raw.startswith(b"<html>")
        assert raw.endswith(b"</html>")
        return ("ja" if provider == "jp" else "en"), parse(raw[6:-7])

    monkeypatch.setattr(source_projection, "project", synthetic_project)
    before = build(recipe, bundle_dir=tmp_path / "before")
    context = InputRecord.model_validate_json(before.input_content).context
    sources = Sources({recipe.store_id: recipe.archive}, recipe.repo, context)
    rules = parse_rules(RULE_CONTENT)
    with open_database(compile_build(SCHEMA), tmp_path / "before/build.sqlite") as db:
        engine = prepare_classifier(
            db, load_glossary(root), sources, case.vocabulary, rules
        )
        units = {row.values["id"]: row.values["text"] for row in db.rows("text_unit")}
        revision = next(
            r.values
            for r in db.rows("face_revision")
            if r.values["region"] == "jp" and units[r.values["effect_unit_id"]] == RAW
        )
        field = OwnerField(
            owner=FaceRevisionOwner(
                kind="face_revision", revision_id=str(revision["id"])
            ),
            field="effect",
            ordinal=None,
        )
        source = descriptor(db, sources, field, recipe.sources[1].card_batch)
        normalized = normalize_source(RAW, source)
        part = normalized.parts[0]
        found = engine.recognize(
            RAW,
            source,
            part,
            field=normalized,
            context=semantic_context(db, card_source(db, sources, source)),
        )
        frame, _ = found.bind(source, part)
    definitions = root / "translations/templates/definitions/000.yaml"
    definitions.parent.mkdir(parents=True)
    definitions.write_bytes(
        canonical(
            {
                "format": 3,
                "kind": "translation_shard",
                "records": [
                    FrameRecord(kind="sentence_template", data=frame).model_dump(
                        mode="json", exclude={"record_key"}
                    )
                ],
            }
        )
    )
    if translated:
        target = TargetRecord.model_validate_json(
            canonical(
                {
                    "kind": "template_translation",
                    "origin": "machine",
                    "low_confidence": True,
                    "data": {
                        "template_id": frame.id,
                        "lang": "zh-Hant",
                        "target": {
                            "format": 1,
                            "nodes": [
                                {"kind": "Literal", "text": "抽"},
                                {"kind": "LeafRef", "slot": "leaf_0"},
                                {"kind": "Literal", "text": "張卡。"},
                            ],
                        },
                    },
                }
            )
        )
        path = root / "translations/templates/values/000.yaml"
        path.parent.mkdir(parents=True)
        path.write_bytes(
            canonical(
                {
                    "format": 3,
                    "kind": "translation_shard",
                    "records": [target.model_dump(mode="json", exclude={"record_key"})],
                }
            )
        )
    after = build(recipe, bundle_dir=tmp_path / "after")
    report = object_value(after.report["effect_translations"])
    assert report["translated"] == int(translated)
    if translated:
        assert report["low_confidence"] == 1
    else:
        assert (
            object_value(report["fallback_reasons"])["missing_template_translation"]
            == 1
        )
    tables = after.projection.tables
    projected = next(r for r in tables["face_revision"] if r["id"] == revision["id"])
    own = [
        object_value(t)
        for t in array(projected["translations"])
        if object_value(t)["field"] == "effect"
    ]
    if translated:
        assert len(own) == 1
        assert own[0]["basis"] == "own_source"
        value = next(
            r for r in tables["translation"] if r["id"] == own[0]["translation_id"]
        )
        text = next(
            r["text"] for r in tables["text_unit"] if r["id"] == value["text_unit_id"]
        )
        assert (text, value["origin"], value["authority"], value["low_confidence"]) == (
            "抽2張卡。",
            "machine",
            "unofficial",
            True,
        )
    else:
        assert own == []
    with open_database(compile_build(SCHEMA), tmp_path / "after/build.sqlite") as db:
        bindings = db.rows("text_template_binding")
        assert len(bindings) == 1
        assert (
            read_binding(db, str(bindings[0].values["id"]), engine.domains).source
            == source
        )
    assert "カード" not in canonical(report).decode()
