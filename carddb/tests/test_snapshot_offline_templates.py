"""Offline builds render whole JP effect fields from templates or keep the original."""

from dataclasses import dataclass, replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build_db.database import open_database
from sve_carddb.build_db.t1 import MINIMUM_CAPABILITIES, compile_build
from sve_carddb.catalog.adoption_models import Batch
from sve_carddb.manifest import Kind
from sve_carddb.snapshot import offline
from sve_carddb.snapshot.offline import build
from sve_carddb.snapshot.values import array, canonical, object_value
from sve_carddb.source_archive import seal_batch
from sve_carddb.sources.official_jp import card_url
from sve_carddb.template_parameter_rules.current import parse as parse_rules
from sve_carddb.template_translations.current import read_templates, validate_templates
from sve_carddb.template_translations.current_build import apply as apply_templates
from sve_carddb.template_translations.current_models import (
    DefinitionRecord,
    Translation,
    TranslationRecord,
)
from sve_carddb.template_translations.current_references import References
from sve_carddb.template_translations.current_sources import Sources

from .adoption_fixtures import commit, git
from .shared_case_fixtures import TextCaseTemplate
from .test_effect_presence import page
from .test_registry import card, make_inputs
from .test_snapshot_offline import prepared as _prepared
from .test_source_archive import _put, _resource, _store
from .test_template_current import _current_definition, _write
from .text_observation_fixtures import make_case

prepared = _prepared

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue

    from sve_carddb.build_db import Database
    from sve_carddb.build_db.database import Row
    from sve_carddb.card_extras import CardPage
    from sve_carddb.snapshot.offline import Built, Inputs
    from sve_carddb.template_translations.current import Validated
    from sve_carddb.template_translations.current_build import Report

    from .text_observation_fixtures import Case

HASH = "sha256:" + "c" * 64
SCHEMA = (
    *MINIMUM_CAPABILITIES,
    "en",
    "translation_evidence",
    "translation_names",
    "translation_templates",
)
TEXTS = {
    "BP02-071": ("名前", "カードを2枚引く。"),
    "PR-001": ("名前", "『名前』を1枚手札に加える。"),
    "BP02-072": ("別名", "『別名』を1枚手札に加える。"),
    "BP02-073": ("第三", "Rule."),
}
# This field has no template source, so it always keeps its original.
UNCOVERED = "BP02-073"


@pytest.fixture(scope="module")
def default_text_case(tmp_path_factory: pytest.TempPathFactory) -> TextCaseTemplate:
    """Give the shared offline fixture Japanese effects that templates can cover."""
    inputs = make_inputs()
    for number, (name, text) in TEXTS.items():
        inputs.jp[number] = card(number, name, text=text)
    base = tmp_path_factory.mktemp("template-text")
    return TextCaseTemplate.capture(make_case(base / "authored", inputs), base)


@dataclass(frozen=True)
class Templates:
    repo: Path
    stores: dict[str, Path]
    batch: Batch


@pytest.fixture(scope="module")
def sources(tmp_path_factory: pytest.TempPathFactory) -> Templates:
    """Seal separate real-format pages; only the exact field hash links them to owners."""
    base = tmp_path_factory.mktemp("template-sources")
    store = _store(base / "store")
    for number, (_, text) in TEXTS.items():
        if number != UNCOVERED:
            raw = page("jp", '<div class="detail">' + text + "</div>")
            _put(
                store,
                _resource(card_url(number), f"raw/{number}.html", raw, Kind.CARD),
                raw,
            )
    batch = Batch(batch_id=seal_batch(store).batch_id)
    repo = base / "repository"
    repo.mkdir()
    git(repo, "init", "-b", "main")
    generated = Sources({store.store_id: store.root}, References(), RULES).generate(
        (batch,)
    )
    definitions: dict[str, DefinitionRecord] = {}
    for member in generated.entries:
        record = _current_definition(member)
        definitions.setdefault(record.data.id, record)
    records: list[DefinitionRecord | TranslationRecord] = sorted(
        [*definitions.values(), *map(translation, definitions.values())],
        key=lambda r: r.record_key,
    )
    files: dict[str, JsonValue] = {
        "translations/templates/current/001.yaml": {
            "translation_authored_format": 2,
            "kind": "translation_shard",
            "records": [r.model_dump(mode="json") for r in records],
        },
    }
    _write(repo, files)
    commit(repo)
    return Templates(repo, {store.store_id: store.root}, batch)


RULES = parse_rules(
    canonical(
        {
            "parameter_rule_format": 2,
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
)


def translation(definition: DefinitionRecord) -> TranslationRecord:
    slots = definition.data.parameter_schema.slots
    number = next(s.name for s in slots if s.type == "uint")
    names = [s.name for s in slots if s.type == "reference"]
    text = (
        "將{{" + number + "}}張『{{" + names[0] + "}}』加入手牌。"
        if names
        else "抽{{" + number + "}}張卡。"
    )
    return TranslationRecord(
        record_key=canonical(
            ["template_translation", definition.data.id, "zh-Hant"]
        ).decode(),
        kind="template_translation",
        data=Translation(template_id=definition.data.id, lang="zh-Hant", text=text),
        origin="machine",
        low_confidence=False,
        note="",
    )


def validated(found: Templates, references: References) -> Validated:
    inputs = read_templates(found.repo, git(found.repo, "rev-parse", "HEAD"))
    return validate_templates(
        inputs, Sources(found.stores, references, RULES), (found.batch,)
    )


def effects(built: Built) -> dict[str, tuple[str | None, bool | None]]:
    """Each JP card's rendered effect text and confidence, or None for the original."""
    tables = built.projection.tables
    units = {row["id"]: row["text"] for row in tables["text_unit"]}
    translations = {row["id"]: row for row in tables["translation"]}
    by_text = {text: number for number, (_, text) in TEXTS.items()}
    result: dict[str, tuple[str | None, bool | None]] = {}
    for revision in tables["face_revision"]:
        number = by_text.get(str(units[revision["effect_unit_id"]]))
        if revision["region"] != "jp" or number is None:
            continue
        bindings = [
            object_value(raw)
            for raw in array(revision["translations"])
            if object_value(raw)["field"] == "effect"
        ]
        assert len(bindings) <= 1
        if not bindings:
            result[number] = (None, None)
            continue
        chosen = translations[bindings[0]["translation_id"]]
        assert (chosen["origin"], chosen["authority"]) == ("machine", "unofficial")
        assert bindings[0]["basis"] == "own_source"
        result[number] = (
            str(units[chosen["text_unit_id"]]),
            bool(chosen["low_confidence"]),
        )
    return result


def test_offline_renders_whole_effects_and_keeps_uncovered_original(
    prepared: tuple[Case, Inputs, tuple[CardPage, ...]],
    sources: Templates,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    _, recipe, _ = prepared
    found = validated(sources, References())
    monkeypatch.setattr(offline, "_templates", lambda *_args, **_kwargs: found)
    populations: list[dict[str, tuple[Row, ...]]] = []

    def capture(db: Database, templates: Validated, lang: str) -> Report:
        report = apply_templates(db, templates, lang)
        populations.append(
            {
                name: db.rows(name)
                for name in (
                    "translation_context",
                    "translation",
                    "translation_selection",
                    "translation_use",
                    "text_template_binding",
                    "translation_binding",
                    "translation_term",
                )
            }
        )
        return report

    monkeypatch.setattr(offline, "apply_templates", capture)
    built = build(recipe, bundle_dir=tmp_path / "bundle")
    assert len(populations) == 1
    assert built.report["effect_translations"] == {
        "fields": 4,
        "translated": 3,
        "original": 1,
        "low_confidence": 2,
        "fallback_reasons": {"unmatched_template_source": 1},
        "pending_parameter_causes": {},
    }
    # Names without a glossary concept keep their source spelling and mark the field.
    assert effects(built) == {
        "BP02-071": ("抽2張卡。", False),
        "PR-001": ("將1張『名前』加入手牌。", True),
        "BP02-072": ("將1張『別名』加入手牌。", True),
        UNCOVERED: (None, None),
    }
    assert "カード" not in canonical(built.report["effect_translations"]).decode()
    with open_database(compile_build(SCHEMA), tmp_path / "bundle/build.sqlite") as db:
        fields = sorted(
            str(row.values["field"])
            for row in db.rows("translation_use")
            if row.values["face_revision_id"] is not None
        )
    assert fields == ["effect"] * 3


@pytest.mark.parametrize(("name", "number"), [("名前", "PR-001"), ("別名", "BP02-072")])
def test_ambiguous_card_name_concept_is_reported_not_resolved(
    prepared: tuple[Case, Inputs, tuple[CardPage, ...]],
    sources: Templates,
    monkeypatch: pytest.MonkeyPatch,
    name: str,
    number: str,
) -> None:
    _, recipe, _ = prepared
    ambiguous = References(card_names={name: ["term:a", "term:b"]})
    found = validated(sources, ambiguous)
    monkeypatch.setattr(offline, "_templates", lambda *_args, **_kwargs: found)
    built = build(recipe)
    report = object_value(built.report["effect_translations"])
    assert (report["translated"], report["original"]) == (2, 2)
    assert report["pending_parameter_causes"] == {"ambiguous_card_name_concept": 1}
    assert effects(built)[number] == (None, None)


def test_pattern_with_only_ambiguous_positions_refuses_the_build(
    sources: Templates,
) -> None:
    pair = ["term:a", "term:b"]
    ambiguous = References(card_names={"名前": pair, "別名": pair})
    with pytest.raises(
        ValueError, match=r"^Template definition source has unresolved parameter roles$"
    ):
        validated(sources, ambiguous)


def test_current_references_fall_back_only_for_a_missing_name() -> None:
    references = References(card_names={"名前": ["term:a"], "別名": ["term:a"] * 2})
    assert references.quoted("名前").target == {
        "kind": "term",
        "id": "term:a",
    }
    missing = references.quoted("第三")
    assert (missing.target, missing.issues) == (
        {"kind": "term", "card_name": "第三"},
        (),
    )
    ambiguous = references.quoted("別名")
    assert (ambiguous.target, ambiguous.issues) == (
        None,
        ("ambiguous_card_name_concept",),
    )


def test_name_fallback_must_carry_its_exact_source_spelling(
    sources: Templates,
) -> None:
    found = validated(sources, References())
    member = next(
        m for m in found.members if any(h.type == "reference" for h in m.hints)
    )
    definition = found.matched_definitions[member.entry.id]
    member.verify_schema(definition.data.parameter_schema)
    tampered = replace(
        member,
        hints=tuple(
            h.model_copy(update={"target": {"kind": "term", "card_name": "他名"}})
            if h.type == "reference"
            else h
            for h in member.hints
        ),
    )
    with pytest.raises(
        ValueError, match=r"^Card-name fallback must keep its exact quoted spelling$"
    ):
        tampered.verify_schema(definition.data.parameter_schema)
