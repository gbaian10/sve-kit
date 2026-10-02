"""Replay signed synthetic glossary choices from a shared sealed API fixture."""

import shutil
from dataclasses import dataclass, replace
from pathlib import Path
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build_inputs import BuildContext
from sve_carddb.catalog.adoption_models import SourceRef
from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.manifest import Kind, Region
from sve_carddb.products.models import LocalizedText
from sve_carddb.snapshot.values import array, canonical, digest, object_value, parse
from sve_carddb.source_archive import ArchiveError, seal_batch
from sve_carddb.text_observations.intern import TextInterner
from sve_carddb.translations.digital import configuration, import_digital
from sve_carddb.translations.importer import Inputs, import_glossary, validate_choice
from sve_carddb.translations.models import ChoiceRecord
from sve_carddb.translations.names import populate_name_translation
from sve_carddb.translations.sources import CODE_PATH, RUNTIME, Sources

from .adoption_fixtures import commit, git
from .test_source_archive import _put, _resource, _store
from .translation_fixtures import choice, envelope, template, term, write

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.build_inputs import Source

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
                store_id=store.store_id,
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
    object_value(definition["data"])["source_ref"] = ja.model_dump(mode="json")
    selected = choice(value="合成乙")
    data = object_value(selected["data"])
    data["origin"] = "official_svwb"
    data["concept_evidence"] = [
        {
            "kind": "dictionary_entry",
            "dictionary_kind": "skill_names",
            "entry_key": "1",
            "jp_ref": ja.model_dump(mode="json"),
            "target_ref": zh.model_dump(mode="json"),
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
                "program_revision": program,
                "code_path": CODE_PATH,
                "code_hash": digest((root / CODE_PATH).read_bytes()),
                "config": {"provider": "svwb"},
                "config_hash": digest(canonical({"provider": "svwb"})),
            }
        }
    }
    config.update(configuration((ja, zh), (("svwb", "22345678"),)))
    config.update(Inputs(root / "authored", root, authored).configuration())
    build = BuildContext.from_inputs(
        program, {name: (root / name).read_bytes() for name in RUNTIME}, config
    )
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
        assert db.rows("glossary_translation")[0].values["origin"] == "official_svwb"
        assert db.rows("glossary_translation")[0].values["text"] == "合成乙"
        assert len(result.uses) == 2
        assert len(db.rows("decision_source")) >= 3
        snapshot = Inputs(frozen.root / "authored", frozen.root, frozen.authored).load()
        for shard in snapshot.envelopes():
            evidence = [
                row.values
                for row in db.rows("decision_source")
                if row.values["decision_id"] == shard.default_decision_id
                and str(row.values["role"]).startswith("translation_evidence:")
            ]
            assert len(evidence) == (
                1 if shard.records[0].kind == "glossary_term" else 2
            )
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
    data["origin"] = "machine"
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
        {name: (repository / name).read_bytes() for name in RUNTIME},
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
        "missing_runtime",
    ],
)
def test_source_and_concept_guards_are_independent(
    frozen: Fixture, fault: str, importer_template: DatabaseTemplate
) -> None:
    inputs = Inputs(frozen.root / "authored", frozen.root, frozen.authored)
    record = next(r for r, _ in inputs.load().records() if isinstance(r, ChoiceRecord))
    content = record.model_dump(mode="json")
    data = object_value(content["data"])
    evidence = object_value(array(data["concept_evidence"])[0])
    build = frozen.build
    if fault == "same_concept":
        data["concept_evidence"] = []
    elif fault == "provider":
        data["origin"] = "official_sv1"
    elif fault == "missing_runtime":
        build = BuildContext.model_validate_json(
            canonical(
                {
                    **build.model_dump(mode="json"),
                    "dependencies": [
                        p.model_dump(mode="json")
                        for p in build.dependencies
                        if p.name != "carddb/src/sve_carddb/translations/names.py"
                    ],
                }
            )
        )
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
    with importer_template.copy() as db, pytest.raises((ValueError, ArchiveError)):
        validate_choice(
            invalid, original="合成甲", sources=frozen.sources(build=build), db=db
        )


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
    data["origin"] = "official_svwb"
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
        "jp_ref": ja.model_dump(mode="json"),
        "target_ref": zh.model_dump(mode="json"),
        "decision_id": "decision",
    }
    if fault == "wrong_face":
        evidence["digital_face_id"] = "digital:svwb:22345678:evolved"
    if fault == "alias_field":
        evidence["target_ref"] = zh.model_copy(
            update={"locator": "/data/card_details/22345678/common/alias"}
        ).model_dump(mode="json")
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
            with pytest.raises(ValueError, match=r"Same-character|locator"):
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
    data["origin"] = "official_svwb"
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
        "jp_ref": ja.model_dump(mode="json"),
        "target_ref": zh.model_dump(mode="json"),
        "jp_span": {"start": 1, "end": 4},
        "target_span": {"start": 1, "end": 4},
        "concept_note": "Synthetic adopted excerpt alignment.",
    }
    if fault == "dictionary_field":
        evidence["jp_ref"] = next(
            r for r in frozen.refs if r.text_hash == digest("合成甲".encode())
        ).model_dump(mode="json")
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
            with pytest.raises(ValueError, match=r"field|Japanese term|span"):
                validate_choice(
                    record,
                    original="其他合成" if fault == "original" else "合成甲",
                    sources=frozen.sources(),
                    db=db,
                )


@pytest.mark.parametrize(
    "fault",
    [
        "none",
        "unadopted",
        "target_changed",
        "unlocated_extra",
        "missing_language_extra",
    ],
)
def test_name_translation_preserves_origin_and_stable_revision(
    frozen: Fixture,
    importer_template: DatabaseTemplate,
    fault: str,
    monkeypatch: pytest.MonkeyPatch,
) -> None:

    with importer_template.copy() as db:
        sources = frozen.sources()
        with db.transaction():
            for identifier in ("svwb:normal", "sv1:normal"):
                db.delete("digital_link", {"id": identifier})
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
                    if fault == "unadopted"
                    else "same_card",
                    "decision_id": "decision",
                },
            )
            if fault in {"unlocated_extra", "missing_language_extra"}:
                db.delete(
                    "digital_text",
                    {"digital_face_id": "svwb:normal", "lang": "zh-Hant"},
                )
                db.insert(
                    "digital_link",
                    {
                        "id": "a-ineligible",
                        "card_id": "card",
                        "face_id": "front",
                        "digital_card_id": "svwb",
                        "digital_face_id": None
                        if fault == "unlocated_extra"
                        else "svwb:normal",
                        "relation": "same_card",
                        "decision_id": "decision",
                    },
                )
            original = TextInterner(db).intern(
                LocalizedText(lang="ja", text="Original synthetic name")
            )
            db.insert(
                "vocabulary",
                {
                    "kind": "type",
                    "code": "follower",
                    "label_unit_id": original,
                    "active": True,
                },
            )
            db.insert(
                "face_revision",
                {
                    "id": "revision",
                    "face_id": "front",
                    "region": "jp",
                    "revision": 1,
                    "temporal_status": "unknown",
                    "observed_at": "2026-10-02T00:00:00Z",
                    "change_kind": "initial",
                    "name_unit_id": original,
                    "effect_unit_id": original,
                    "type_code": "follower",
                    "source_id": "source",
                },
            )
        if fault == "target_changed":
            # Bypass the lower text resolver to prove the name check independently.
            def permissive_text(
                ref: SourceRef, _span: object = None
            ) -> tuple[str, str, Source]:
                lang, _, source = sources.document(ref)
                return lang, "Changed synthetic name", source

            monkeypatch.setattr(sources, "text", permissive_text)
            with db.transaction():
                unit = next(
                    r.values["name_unit_id"]
                    for r in db.rows("digital_text")
                    if r.values["digital_face_id"] == "digital:svwb:22345678:normal"
                    and r.values["lang"] == "zh-Hant"
                )
                db.update(
                    "text_unit",
                    {"id": unit},
                    {
                        "text": "Changed synthetic name",
                        "content_hash": digest(b"Changed synthetic name"),
                    },
                )
            with (
                pytest.raises(ValueError, match="differs from frozen"),
                db.transaction(),
            ):
                populate_name_translation(
                    db, sources, revision_id="revision", lang="zh-Hant"
                )
        else:
            with db.transaction():
                first = populate_name_translation(
                    db, sources, revision_id="revision", lang="zh-Hant"
                )
                second = populate_name_translation(
                    db, sources, revision_id="revision", lang="zh-Hant"
                )
            assert first == second
            if fault == "unadopted":
                assert first is None
                assert not db.rows("translation")
                assert not db.rows("translation_context")
            else:
                row = db.rows("translation")[0].values
                assert row["origin"] == "official_svwb"
                assert row["authority"] == "digital_official"
                assert row["tokens"] is None
                assert row["translated_at"] == "2026-10-02T00:00:00Z"
                assert row["revision"] == int(str(row["id"])[3:16], 16)
                source = next(
                    r.values
                    for r in db.rows("source_record")
                    if r.values["id"] == row["source_id"]
                )
                assert "lang=cht" in str(source["url"])
                assert not db.has_table("translation_selection")
