"""Replay signed synthetic glossary choices from a shared sealed API fixture."""

import shutil
from dataclasses import dataclass, replace
from pathlib import Path
from typing import TYPE_CHECKING

import pytest

from sve_carddb.catalog.adoption_models import SourceRef
from sve_carddb.core.json import array, canonical, digest, object_value, parse
from sve_carddb.core.provenance import BuildContext
from sve_carddb.ingest.archive.frozen_sources import FrozenSources
from sve_carddb.ingest.archive.manifest import Kind, Region
from sve_carddb.ingest.archive.source_archive import ArchiveError, seal_batch
from sve_carddb.translations.current_models import ChoiceRecord, DigitalName
from sve_carddb.translations.digital import (
    _phases,
    configuration,
    import_digital,
    name_proof,
)
from sve_carddb.translations.importer import (
    Inputs,
    _digital_evidence,
    _digital_location,
    import_glossary,
    validate_choice,
)
from sve_carddb.translations.sources import Sources

from .adoption_fixtures import commit, git
from .test_source_archive import _put, _resource, _store
from .translation_fixtures import choice, envelope, template, term, write

CODE_PATH = "carddb/src/sve_carddb/translations/sources.py"
RUNTIME = (CODE_PATH,)


if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.core.provenance import Source
    from sve_carddb.ingest.archive.source_archive import Descriptor

    from .database_fixtures import DatabaseTemplate


@dataclass(frozen=True)
class Fixture:
    root: Path
    store: Path
    program: str
    authored: str
    refs: tuple[SourceRef, ...]
    build: BuildContext

    def sources(
        self, *, build: BuildContext | None = None, store: Path | None = None
    ) -> Sources:
        """Create an isolated resolver over an immutable module fixture."""
        return Sources(
            {"test-store": store or self.store}, self.root, build or self.build
        )


@pytest.fixture(scope="module")
def importer_template() -> DatabaseTemplate:
    return template()


@pytest.fixture(scope="module")
def frozen(  # ruff: ignore[too-many-locals] -- two sealed language sources share one module baseline
    tmp_path_factory: pytest.TempPathFactory,
) -> Fixture:
    root = tmp_path_factory.mktemp("translation-frozen")
    repository = Path(__file__).resolve().parents[2]
    for name in RUNTIME:
        target = root / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(repository / name, target)
    git(root, "init")
    program = commit(root)
    store = _store(root / "fixture-store")
    for lang, word in (("ja", "合成甲"), ("cht", "合成乙")):
        raw = canonical(
            {
                "data_headers": {"result_code": 1},
                "data": {
                    "skill_names": {"1": word, "2": "合成丙"},
                    "tribe_names": {},
                    "card_details": {
                        "22345678": {
                            "common": {
                                "card_id": 22345678,
                                "name": "Synthetic " + lang,
                                "alias": "Synthetic " + lang,
                                "skill_text": "x" + word + "x",
                                "base_card_id": 22345678,
                                "original_card_id": None,
                                "is_token": False,
                            },
                            "evo": {"skill_text": "Synthetic back effect"},
                        }
                    },
                },
            }
        )
        resource = replace(
            _resource(
                "https://shadowverse-wb.com/web/CardList/cardList?lang="
                + lang
                + "&offset=0",
                "raw/" + lang + ".json",
                raw,
                Kind.API,
            ),
            region=Region.SVWB,
            content_type="application/json",
        )
        _put(store, resource, raw)
    sealed = seal_batch(store)
    sources = FrozenSources(store.root, store.store_id, sealed.batch_id)
    refs: list[SourceRef] = []
    for entry in sources.inventory.current:
        source, raw, _ = sources.read(
            entry.source_version_id, parser_version="translation-svwb-v1"
        )
        words = object_value(
            object_value(object_value(parse(raw))["data"])["skill_names"]
        )
        refs.append(
            SourceRef(
                batch_id=sealed.batch_id,
                source_version_id=source.id,
                parser="translation-svwb-v1",
                locator="/data/skill_names/1",
                text_hash=digest(str(words["1"]).encode()),
            )
        )
    refs.sort(key=lambda r: sources.descriptor(r.source_version_id).url)
    ja = next(
        r for r in refs if "lang=ja" in sources.descriptor(r.source_version_id).url
    )
    zh = next(
        r for r in refs if "lang=cht" in sources.descriptor(r.source_version_id).url
    )
    definition = term()
    object_value(definition["data"])["source_ref"] = ja.model_dump(
        mode="json", round_trip=True
    )
    selected = choice(value="合成乙")
    data = object_value(selected["data"])
    selected["origin"] = "official"
    data["concept_evidence"] = [
        {
            "kind": "dictionary_entry",
            "dictionary_kind": "skill_names",
            "entry_key": "1",
            "jp_ref": ja.model_dump(mode="json", round_trip=True),
            "target_ref": zh.model_dump(mode="json", round_trip=True),
            "concept_note": "Same synthetic dictionary key.",
        }
    ]
    write(
        root / "authored",
        {
            "translations/glossary/concepts/001.yaml": envelope([definition]),
            "translations/glossary/choices/001.yaml": envelope([selected]),
        },
    )
    authored = commit(root)
    config: dict[str, JsonValue] = {
        "translation_recipes": {
            "translation-svwb-v1": {
                "version": "translation-svwb-v1",
                "config": {"provider": "svwb"},
            }
        }
    }
    config.update(configuration((ja, zh), (("svwb", "22345678"),)))
    config.update(Inputs(root / "authored", root, authored).configuration())
    build = BuildContext.from_inputs(program, config)
    return Fixture(root, store.root, program, authored, (ja, zh), build)


def test_frozen_glossary_import_and_f1(
    frozen: Fixture, importer_template: DatabaseTemplate
) -> None:
    with importer_template.copy() as db:
        result = import_glossary(
            db,
            Inputs(frozen.root / "authored", frozen.root, frozen.authored),
            build=frozen.build,
            stores={"test-store": frozen.store},
        )
        assert db.rows("glossary_term")[0].values["source_ja"] == "合成甲"
        assert db.rows("glossary_translation")[0].values["origin"] == "official"
        assert db.rows("glossary_translation")[0].values["text"] == "合成乙"
        assert len(result.uses) == 2
        assert all(
            row.values["authored_source_id"] is not None
            for row in db.rows("glossary_term")
        )
        assert len(db.rows("decision")) == 1
        assert not db.rows("translation")


def test_human_review_preserves_machine_origin(
    frozen: Fixture, importer_template: DatabaseTemplate, tmp_path: Path
) -> None:
    repository = tmp_path / "repository"
    shutil.copytree(frozen.root, repository)
    snapshot = Inputs(repository / "authored", repository, frozen.authored).load()
    shards = {
        path: object_value(parse(content)) for path, _, content in snapshot.shards
    }
    name = "translations/glossary/choices/001.yaml"
    record = object_value(array(shards[name]["records"])[0])
    data = object_value(record["data"])
    record["origin"] = "machine"
    data["value"] = {"kind": "authored", "text": "Synthetic machine translation"}
    data["concept_evidence"] = []
    shards[name] = envelope([record])
    write(repository / "authored", shards)
    revision = commit(repository)
    inputs = Inputs(repository / "authored", repository, revision)
    config = object_value(parse(frozen.build.configuration.encode()))
    config.update(inputs.configuration())
    build = BuildContext.from_inputs(
        frozen.program,
        config,
    )
    with importer_template.copy() as db:
        import_glossary(db, inputs, build=build, stores={"test-store": frozen.store})
        assert db.rows("glossary_translation")[0].values["origin"] == "machine"


def test_digital_parent_back_face_and_language_closure(
    frozen: Fixture, importer_template: DatabaseTemplate
) -> None:
    with importer_template.copy() as db, db.transaction():
        sources = frozen.sources()
        import_digital(db, sources, frozen.refs, (("svwb", "22345678"),))
        faces = [
            r
            for r in db.rows("digital_face")
            if str(r.values["id"]).startswith("digital:")
        ]
        assert {r.values["phase"] for r in faces} == {"normal", "evolved"}
        names = [
            r
            for r in db.rows("digital_text")
            if str(r.values["digital_face_id"]).startswith("digital:")
        ]
        assert len(names) == 4
        assert all(r.values["effect_unit_id"] is None for r in names)
        assert not db.has_table("digital_art")
        assert not db.has_table("voice")


@pytest.mark.parametrize(
    "fault",
    [
        "missing_version",
        "field_hash",
        "pointer",
        "target_language",
        "dictionary_key",
        "same_concept",
        "provider",
    ],
)
def test_source_and_concept_guards_are_independent(
    frozen: Fixture,
    fault: str,
    importer_template: DatabaseTemplate,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    inputs = Inputs(frozen.root / "authored", frozen.root, frozen.authored)
    record = next(
        r for r in inputs.load().current_records() if isinstance(r, ChoiceRecord)
    )
    content = record.model_dump(mode="json", round_trip=True)
    data = object_value(content["data"])
    evidence = object_value(array(data["concept_evidence"])[0])
    if fault == "same_concept":
        data["concept_evidence"] = []
    elif fault == "provider":
        evidence["target_ref"] = object_value(evidence["target_ref"]) | {
            "parser": "translation-jp-v1"
        }
    else:
        ref = object_value(evidence["target_ref"])
        if fault == "missing_version":
            ref["source_version_id"] = "src:v1:" + "0" * 64
        elif fault == "field_hash":
            ref["text_hash"] = "sha256:" + "0" * 64
        elif fault == "pointer":
            ref["locator"] = "/data/missing"
        elif fault == "target_language":
            evidence["target_ref"] = evidence["jp_ref"]
        elif fault == "dictionary_key":
            evidence["entry_key"] = "2"
    invalid = ChoiceRecord.model_validate_json(canonical(content))
    messages = {
        "missing_version": "^Source version is absent from pinned batch$",
        "field_hash": "^Evidence must locate exact hash-verified text$",
        "pointer": "^Evidence JSON Pointer is absent$",
        "target_language": "^Concept evidence language/exact target mismatch$",
        "dictionary_key": "^Official dictionary concept/key mismatch$",
        "same_concept": "^Official choice lacks same-concept evidence$",
        "provider": "^Official origin differs from evidence provider$",
    }
    sources = frozen.sources()
    if fault == "provider":
        source = frozen.sources().text(frozen.refs[1])[2]
        monkeypatch.setattr(
            sources,
            "text",
            lambda ref, _span=None: (
                ("ja", "合成甲", source)
                if ref == frozen.refs[0]
                else ("zh-Hant", "合成乙", source)
            ),
        )
    with (
        importer_template.copy() as db,
        pytest.raises((ValueError, ArchiveError), match=messages[fault]),
    ):
        validate_choice(invalid, original="合成甲", sources=sources, db=db)


def test_raw_tamper_never_falls_back_to_draft(frozen: Fixture, tmp_path: Path) -> None:
    store = tmp_path / "store"
    shutil.copytree(frozen.store, store)
    raw = next((store / "raw").rglob("*.raw"))
    raw.write_bytes(b"Synthetic tampering")
    with pytest.raises(ArchiveError):
        frozen.sources(store=store).text(frozen.refs[0])


@pytest.mark.parametrize(
    "fault", ["none", "same_character", "wrong_face", "alias_field", "unadopted"]
)
def test_digital_name_requires_exact_adopted_face(
    frozen: Fixture, importer_template: DatabaseTemplate, fault: str
) -> None:
    content = choice(value="Synthetic cht")
    data = object_value(content["data"])
    content["origin"] = "official"
    ja = next(
        r for r in frozen.refs if r.text_hash == digest("合成甲".encode())
    ).model_copy(
        update={
            "locator": "/data/card_details/22345678/common/name",
            "text_hash": digest(b"Synthetic ja"),
        }
    )
    zh = next(
        r for r in frozen.refs if r.text_hash == digest("合成乙".encode())
    ).model_copy(
        update={
            "locator": "/data/card_details/22345678/common/name",
            "text_hash": digest(b"Synthetic cht"),
        }
    )
    evidence: dict[str, JsonValue] = {
        "kind": "digital_name",
        "digital_face_id": "digital:svwb:22345678:normal",
        "sve_owner": "front",
        "jp_ref": ja.model_dump(mode="json", round_trip=True),
        "target_ref": zh.model_dump(mode="json", round_trip=True),
    }
    if fault == "wrong_face":
        evidence["digital_face_id"] = "digital:svwb:22345678:evolved"
    if fault == "alias_field":
        evidence["target_ref"] = zh.model_copy(
            update={"locator": "/data/card_details/22345678/common/alias"}
        ).model_dump(mode="json", round_trip=True)
    data["concept_evidence"] = [evidence]
    record = ChoiceRecord.model_validate_json(canonical(content))
    with importer_template.copy() as db:
        with db.transaction():
            sources = frozen.sources()
            import_digital(db, sources, frozen.refs, (("svwb", "22345678"),))
            db.insert(
                "digital_link",
                {
                    "id": "new-link",
                    "card_id": "card",
                    "face_id": "front",
                    "digital_card_id": "digital:svwb:22345678",
                    "digital_face_id": "digital:svwb:22345678:normal",
                    "relation": "same_character"
                    if fault == "same_character"
                    else "same_card",
                    "decision_id": "decision",
                },
            )
        if fault == "unadopted":
            with db.transaction():
                db.delete("digital_link", {"id": "new-link"})
            with pytest.raises(ValueError, match="Same-character"):
                validate_choice(
                    record, original="Synthetic ja", sources=frozen.sources(), db=db
                )
        elif fault == "none":
            selected = validate_choice(
                record, original="Synthetic ja", sources=frozen.sources(), db=db
            )
            assert selected is not None
            assert selected[0] == "Synthetic cht"
        else:
            with pytest.raises(
                ValueError,
                match={
                    "same_character": "^Same-character/name-only is not same-concept name evidence$",
                    "wrong_face": "^Same-character/name-only is not same-concept name evidence$",
                    "alias_field": "^Digital name locator points to another card or field$",
                }[fault],
            ):
                validate_choice(
                    record, original="Synthetic ja", sources=frozen.sources(), db=db
                )


@pytest.mark.parametrize(
    "fault", ["none", "dictionary_field", "original", "outside_span"]
)
def test_effect_excerpt_has_exact_role_and_concept(
    frozen: Fixture, importer_template: DatabaseTemplate, fault: str
) -> None:
    content = choice(value="合成乙")
    data = object_value(content["data"])
    content["origin"] = "official"
    ja = next(
        r for r in frozen.refs if r.text_hash == digest("合成甲".encode())
    ).model_copy(
        update={
            "locator": "/data/card_details/22345678/common/skill_text",
            "text_hash": digest("x合成甲x".encode()),
        }
    )
    zh = next(
        r for r in frozen.refs if r.text_hash == digest("合成乙".encode())
    ).model_copy(
        update={
            "locator": "/data/card_details/22345678/common/skill_text",
            "text_hash": digest("x合成乙x".encode()),
        }
    )
    evidence: dict[str, JsonValue] = {
        "kind": "effect_term",
        "jp_ref": ja.model_dump(mode="json", round_trip=True),
        "target_ref": zh.model_dump(mode="json", round_trip=True),
        "jp_span": {"start": 1, "end": 4},
        "target_span": {"start": 1, "end": 4},
        "concept_note": "Synthetic adopted excerpt alignment.",
    }
    if fault == "dictionary_field":
        evidence["jp_ref"] = next(
            r for r in frozen.refs if r.text_hash == digest("合成甲".encode())
        ).model_dump(mode="json", round_trip=True)
        evidence["jp_span"] = {"start": 0, "end": 3}
    elif fault == "outside_span":
        evidence["target_span"] = {"start": 1, "end": 6}
    data["concept_evidence"] = [evidence]
    record = ChoiceRecord.model_validate_json(canonical(content))
    with importer_template.copy() as db:
        if fault == "none":
            selected = validate_choice(
                record, original="合成甲", sources=frozen.sources(), db=db
            )
            assert selected is not None
            assert selected[0] == "合成乙"
        else:
            with pytest.raises(
                ValueError,
                match={
                    "dictionary_field": "^Effect-term evidence must locate an effect field$",
                    "original": "^Concept evidence differs from adopted Japanese term$",
                    "outside_span": "^Invalid exact source span$",
                }[fault],
            ):
                validate_choice(
                    record,
                    original="其他合成" if fault == "original" else "合成甲",
                    sources=frozen.sources(),
                    db=db,
                )


def changed_build(frozen: Fixture, config: dict[str, JsonValue]) -> BuildContext:
    return BuildContext.from_inputs(
        frozen.program,
        config,
    )


@pytest.mark.parametrize("fault", ["configuration", "japanese"])
def test_immutable_authored_and_atomic_projection(
    frozen: Fixture, importer_template: DatabaseTemplate, tmp_path: Path, fault: str
) -> None:
    repository = tmp_path / "repository"
    shutil.copytree(frozen.root, repository)
    inputs = Inputs(repository / "authored", repository, frozen.authored)
    config = object_value(parse(frozen.build.configuration.encode()))
    message = ""
    if fault == "configuration":
        config.pop("translation_authored")
        message = "Build configuration does not pin translation authored bytes"
    else:
        snapshot = inputs.load()
        shards = {
            name: object_value(parse(content)) for name, _, content in snapshot.shards
        }
        name = "translations/glossary/concepts/001.yaml"
        record = object_value(array(shards[name]["records"])[0])
        object_value(record["data"])["source_ref"] = frozen.refs[1].model_dump(
            round_trip=True, mode="json"
        )
        shards[name] = envelope([record])
        message = "Glossary concept requires exact Japanese source"
        write(repository / "authored", shards)
        inputs = replace(inputs, authored_revision=commit(repository))
        config.update(inputs.configuration())
    with importer_template.copy() as db:
        before = {name: db.rows(name) for name in db._tables}
        with pytest.raises(ValueError, match="^" + message + "$"):
            import_glossary(
                db,
                inputs,
                build=changed_build(frozen, config),
                stores={"test-store": frozen.store},
            )
        assert {name: db.rows(name) for name in db._tables} == before


@pytest.mark.parametrize(
    ("fault", "message"),
    [
        ("language", "Choice source language mismatch"),
        ("outside_evidence", "Official source value is outside its concept evidence"),
        ("japanese_evidence", "Concept evidence language/exact target mismatch"),
    ],
)
def test_choice_source_matches_adopted_evidence(
    frozen: Fixture, importer_template: DatabaseTemplate, fault: str, message: str
) -> None:
    record = next(
        r
        for r in Inputs(frozen.root / "authored", frozen.root, frozen.authored)
        .load()
        .current_records()
        if isinstance(r, ChoiceRecord)
    )
    content = record.model_dump(mode="json", round_trip=True)
    data = object_value(content["data"])
    ref = frozen.refs[1]
    if fault == "language":
        ref = frozen.refs[0]
    elif fault == "outside_evidence":
        # The same target word in another exact field does not prove this value's concept.
        ref = ref.model_copy(
            update={
                "locator": "/data/card_details/22345678/common/skill_text",
                "text_hash": digest("x合成乙x".encode()),
            }
        )
    else:
        object_value(array(data["concept_evidence"])[0])["jp_ref"] = frozen.refs[
            1
        ].model_dump(mode="json", round_trip=True)
    data["value"] = {
        "kind": "source",
        "source_ref": ref.model_dump(mode="json", round_trip=True),
        "span": {"start": 1, "end": 4} if fault == "outside_evidence" else None,
    }
    with (
        importer_template.copy() as db,
        pytest.raises(ValueError, match="^" + message + "$"),
    ):
        validate_choice(
            ChoiceRecord.model_validate_json(canonical(content)),
            original="合成甲",
            sources=frozen.sources(),
            db=db,
        )


@pytest.mark.parametrize(
    ("fault", "message"),
    [
        ("char_type", "Invalid sv1 character type"),
        ("extra", "Unrecognized extra digital face requires parser verification"),
        ("evolved", "Invalid svwb evolved face"),
    ],
)
def test_recognized_face_layouts_only(fault: str, message: str) -> None:
    with pytest.raises(ValueError, match="^" + message + "$"):
        _phases(
            "sv1" if fault == "char_type" else "svwb",
            {"char_type": True},
            {"super_evo": {}}
            if fault == "extra"
            else {"evo": "synthetic invalid face"},
        )


@pytest.mark.parametrize(
    ("fault", "message"),
    [
        ("pins", "Build configuration does not pin digital inputs"),
        ("provider", "Digital closure requires frozen API sources"),
        ("id_type", "Digital card lacks an official integer ID"),
        ("conflict", "Conflicting frozen digital card versions"),
        ("width", "Digital official ID width mismatch"),
        ("missing", "Requested digital card has no frozen Japanese source"),
        ("parent_missing", "Requested digital card has no frozen Japanese source"),
        ("parent_type", "Invalid digital parent ID"),
        ("token", "Invalid digital token flag"),
        ("localized_faces", "Digital localized face closure mismatch"),
    ],
)
def test_digital_input_closure_refusals(  # ruff: ignore[complex-structure] -- mutate one API projection while preserving all lower frozen checks
    frozen: Fixture,
    importer_template: DatabaseTemplate,
    monkeypatch: pytest.MonkeyPatch,
    fault: str,
    message: str,
) -> None:
    sources = frozen.sources()
    refs = frozen.refs
    targets = (("svwb", "22345678"),)
    if fault == "width":
        targets = (("svwb", "2234567"),)
    elif fault == "missing":
        targets = (("svwb", "32345678"),)
    elif fault == "provider":
        refs = (refs[0].model_copy(update={"parser": "translation-jp-v1"}),)
    elif fault == "conflict":
        refs = (
            *refs,
            refs[0].model_copy(update={"locator": "/synthetic-conflicting-version"}),
        )
    config = object_value(parse(frozen.build.configuration.encode()))
    config.update(configuration(refs, targets))
    if fault == "pins":
        config.pop("digital_evidence")
    sources = sources.stage(changed_build(frozen, config))
    document = sources.document

    def projected(ref: SourceRef) -> tuple[str, JsonValue, Source]:
        real = ref.model_copy(update={"parser": "translation-svwb-v1"})
        lang, original, source = document(real)
        value = parse(canonical(original))
        item = object_value(object_value(object_value(value)["data"])["card_details"])[
            "22345678"
        ]
        common = object_value(object_value(item)["common"])
        if fault == "id_type":
            common["card_id"] = "22345678"
        elif fault == "conflict" and ref.locator == "/synthetic-conflicting-version":
            common["name"] = "Conflicting synthetic version"
        elif fault == "parent_type":
            common["base_card_id"] = "32345678"
        elif fault == "parent_missing":
            common["original_card_id"] = 32345678
        elif fault == "token":
            common["is_token"] = 1
        elif fault == "localized_faces" and lang == "zh-Hant":
            object_value(item)["evo"] = []
        return lang, value, source

    monkeypatch.setattr(sources, "document", projected)
    with importer_template.copy() as db:
        with pytest.raises(ValueError, match="^" + message + "$"), db.transaction():
            import_digital(db, sources, refs, targets)


@pytest.mark.parametrize(
    ("fault", "message"),
    [
        ("recipe", "Unsupported translation source recipe"),
        ("descriptor_provider", "Frozen evidence provider/kind mismatch"),
        ("descriptor_kind", "Frozen evidence provider/kind mismatch"),
    ],
)
def test_source_recipe_and_descriptor_refusals(
    frozen: Fixture, monkeypatch: pytest.MonkeyPatch, fault: str, message: str
) -> None:
    if fault == "recipe":
        sources = frozen.sources()
        with pytest.raises(ValueError, match=message):
            sources.document(
                frozen.refs[0].model_copy(update={"parser": "unsupported-v1"})
            )
        return
    sources = frozen.sources()
    if fault.startswith("descriptor"):
        read = FrozenSources.read

        def inconsistent_descriptor(
            self: FrozenSources, version: str, *, parser_version: str
        ) -> tuple[Source, bytes, Descriptor]:
            source, raw, descriptor = read(self, version, parser_version=parser_version)
            descriptor = descriptor.model_copy(
                update={"provider": "jp"}
                if fault == "descriptor_provider"
                else {"kind": "card"}
            )
            return source, raw, descriptor

        monkeypatch.setattr(FrozenSources, "read", inconsistent_descriptor)
    with pytest.raises(ValueError, match="^" + message + "$"):
        sources.document(frozen.refs[0])


@pytest.mark.parametrize(
    ("fault", "message"),
    [
        ("provider", "Digital name locator provider mismatch"),
        ("field", "Digital name locator must reference a card name"),
        ("card", "Digital name locator points to another card"),
        ("decision", "Same-character/name-only is not same-concept name evidence"),
        ("names", "Digital name evidence does not locate the adopted face names"),
    ],
)
def test_digital_concept_location_guards(
    frozen: Fixture,
    importer_template: DatabaseTemplate,
    monkeypatch: pytest.MonkeyPatch,
    fault: str,
    message: str,
) -> None:
    ref = frozen.refs[0]
    face = "sv1:normal" if fault in {"provider", "field", "card"} else "svwb:normal"
    evidence = DigitalName(
        kind="digital_name",
        digital_face_id=face,
        sve_owner="front",
        jp_ref=ref,
        target_ref=frozen.refs[1],
    )
    sources = frozen.sources()
    if fault in {"field", "card"}:
        source = sources.document(ref)[2]
        ref = ref.model_copy(
            update={
                "parser": "translation-sv1-v1",
                "locator": "/data/cards/0/alias"
                if fault == "field"
                else "/data/cards/0/card_name",
            }
        )
        monkeypatch.setattr(
            sources,
            "document",
            lambda _ref: (
                "ja",
                {"data": {"cards": [{"card_id": 987654321, "card_name": "Synthetic"}]}},
                source,
            ),
        )
    with (  # ruff: ignore[pytest-raises-with-multiple-statements] -- test either independent location or concept boundary
        importer_template.copy() as db,
        pytest.raises(ValueError, match="^" + message + "$"),
    ):
        if fault in {"decision", "names"}:
            if fault == "decision":
                with db.transaction():
                    db.delete("digital_link", {"id": "svwb:normal"})
            _digital_evidence(
                evidence, "Wrong synthetic name", "svwb:normal zh-Hant", "zh-Hant", db
            )
        else:
            _digital_location(evidence, ref, sources, db)


@pytest.mark.parametrize(
    ("phase", "lang", "message"),
    [
        ("super", "zh-Hant", "Selected digital face is absent from frozen source"),
        ("normal", "en", "Selected digital name lacks frozen language evidence"),
    ],
)
def test_name_has_frozen_phase_and_language(
    frozen: Fixture, phase: str, lang: str, message: str
) -> None:
    with pytest.raises(ValueError, match="^" + message + "$"):
        name_proof(frozen.sources(), "svwb", "22345678", phase, lang, "Synthetic cht")


@pytest.mark.parametrize("bold", [True, False, None])
def test_project_receipt_and_rawless_concept_import_atomically(
    frozen: Fixture,
    importer_template: DatabaseTemplate,
    tmp_path: Path,
    bold: bool | None,
) -> None:
    from .test_glossary_adoption import authored, claim, emphasis  # ruff: ignore[import-outside-top-level] -- share only small envelope builders, not another frozen fixture

    repository = tmp_path / "project-repository"
    shutil.copytree(frozen.root, repository)
    definition = authored()
    selected = choice(value="合成專案譯名")
    object_value(selected["data"])["source_claim"] = claim()
    records = {
        "translations/glossary/concepts/001.yaml": envelope([definition]),
        "translations/glossary/choices/001.yaml": envelope([selected]),
        "translations/glossary/emphasis/001.yaml": envelope([emphasis(value=bold)]),
    }
    write(repository / "authored", records)
    inputs = Inputs(repository / "authored", repository, commit(repository))
    config = object_value(parse(frozen.build.configuration.encode()))
    config.update(inputs.configuration())
    with importer_template.copy() as db:
        imported = import_glossary(
            db, inputs, build=changed_build(frozen, config), stores={}
        )
        assert imported is not None
        assert db.rows("glossary_term")[0].values["source_ja"] == "合成名"
        assert db.rows("glossary_translation")[0].values["origin"] == "project"
        assert db.rows("glossary_translation")[0].values["text"] == "合成專案譯名"
        assert db.rows("glossary_term")[0].values["emphasis"] is bold
        assert len(db.rows("decision")) == 1
        audit = [
            row
            for row in db.rows("source_record")
            if row.values["authored_path"] is not None
        ]
        assert len(audit) == 3
        assert all(row.values["kind"] == "authored" for row in audit)


def test_source_stages_share_reads_without_accumulating_uses(
    frozen: Fixture, monkeypatch: pytest.MonkeyPatch
) -> None:
    parent = frozen.sources()
    expected = parent.text(frozen.refs[0])
    original_uses = tuple(parent.uses)
    context = BuildContext.from_inputs(frozen.program, {"phase": "names"})
    first = parent.stage(context)
    second = parent.stage(frozen.build)

    def unexpected_batch(_self: Sources, _batch: str) -> FrozenSources:
        pytest.fail("Stage must reuse the already verified source projection")

    monkeypatch.setattr(Sources, "batch", unexpected_batch)
    assert first.text(frozen.refs[0]) == second.text(frozen.refs[0]) == expected
    assert tuple(parent.uses) == original_uses
    assert first.uses == second.uses == list(original_uses)
    first.uses.clear()
    assert second.uses == parent.uses == list(original_uses)
    assert first.build == context
    assert second.build == parent.build == frozen.build
