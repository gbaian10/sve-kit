"""Mechanism facts come from admitted programs; unchecked cards never read as absent."""

import sqlite3
from contextlib import contextmanager
from dataclasses import replace
from typing import TYPE_CHECKING, Literal

import pytest
from pydantic import JsonValue, ValidationError

from sve_carddb.build import Json, create_database
from sve_carddb.core.json import array, canonical, digest, object_value
from sve_carddb.domains import mechanic_coverage, mechanics
from sve_carddb.domains.products.models import LocalizedText
from sve_carddb.domains.registry.records import FaceData
from sve_carddb.domains.registry.snapshot import load_registry
from sve_carddb.domains.text_observations.intern import TextInterner

from ..support import digital_link_import_fixtures
from ..support.compiled_schemas import compile_build
from ..support.digital_link_import_fixtures import make_fixture
from ..support.translation_fixtures import template

if TYPE_CHECKING:
    from collections.abc import Generator, Mapping
    from pathlib import Path

    from sve_carddb.build import Database, Value

REVISION = "0" * 40
_SPELLINGS = {
    "ward": "守護",
    "storm": "疾走",
    "rush": "突進",
    "bane": "必殺",
    "assail": "指定攻撃",
    "intimidate": "威圧",
    "aura": "オーラ",
    "drain": "ドレイン",
}
_PORTS: dict[str, JsonValue] = {"exports": {}, "imports": {}}


def registry() -> mechanics.Registry:
    """The eight tracked keywords plus one concept outside program derivation."""
    records: list[JsonValue] = [
        {"id": "kw:" + code, "code": code, "term_id": "term:keyword." + code}
        for code in _SPELLINGS
    ]
    records.append(
        {
            "id": "kw:quick",
            "code": "quick",
            "term_id": "term:ability.quick",
            "term_category": "ability",
        }
    )
    return mechanics.Registry.model_validate_json(
        canonical(
            {
                "format": 1,
                "kind": "keyword_registry",
                "decision": {
                    "id": "mapping:synthetic",
                    "state": "confirmed",
                    "note": "Synthetic mapping.",
                },
                "records": records,
            }
        )
    )


@contextmanager
def case(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    raw: str,
    *,
    back: str | None = None,
    sections: tuple[str, ...] = (),
) -> Generator[tuple[Database, mechanics.Inputs, str]]:
    html = digital_link_import_fixtures.RAW
    if back is not None:
        start = html.index(b'<div class="cardlist-Detail_Box_Inner">')
        end = html.index(b"\n<!--") - len(b"</div>")
        face = html[start:end]
        html = (
            html[:start]
            + face
            + face.replace(b"Synthetic rule.", back.encode())
            + html[end:]
        )
    monkeypatch.setattr(
        digital_link_import_fixtures,
        "RAW",
        html.replace(
            b"Synthetic rule.",
            (raw + "".join("\n-----\n" + section for section in sections))
            .replace("\n", "<br>")
            .encode(),
        ),
    )
    fixture = make_fixture(tmp_path)
    faces = sorted(
        (
            r.data
            for r in load_registry(fixture.root / "authored").records.values()
            if isinstance(r.data, FaceData) and r.data.card_id == fixture.card.id
        ),
        key=lambda face: face.ordinal,
    )
    fixture = replace(fixture, face=faces[0])
    inputs = mechanics.Inputs(registry(), canonical(registry().model_dump(mode="json")))
    schema = compile_build(("keyword", "correction", "dsl"))
    with create_database(schema) as db, template().copy() as baseline:
        with db.transaction():
            for table in template().schema.tables:
                for row in baseline.rows(table.name):
                    db.insert(table.name, row.values)
        fixture.publish(db)
        with db.transaction():
            unit = TextInterner(db).intern(LocalizedText(lang="ja", text=raw))
            db.update(
                "face_revision", {"id": "link-revision"}, {"effect_unit_id": unit}
            )
            db.insert(
                "face_current",
                {
                    "face_id": fixture.face.id,
                    "region": "jp",
                    "revision_id": "link-revision",
                    "basis": "latest_observed_no_errata",
                },
            )
            for ordinal, text in enumerate(sections):
                db.insert(
                    "face_text_section",
                    {
                        "revision_id": "link-revision",
                        "ordinal": ordinal,
                        "text_unit_id": TextInterner(db).intern(
                            LocalizedText(lang="ja", text=text)
                        ),
                        "kind": "unknown",
                    },
                )
            if back is not None:
                db.insert("face", faces[1].model_dump(mode="json", round_trip=True))
                revision = db.select(
                    "face_revision",
                    db.columns("face_revision"),
                    where={"id": "link-revision"},
                )[0].values
                db.insert(
                    "face_revision",
                    dict(revision)
                    | {
                        "id": "back-revision",
                        "face_id": faces[1].id,
                        "effect_unit_id": TextInterner(db).intern(
                            LocalizedText(lang="ja", text=back)
                        ),
                    },
                )
                db.insert(
                    "face_current",
                    {
                        "face_id": faces[1].id,
                        "region": "jp",
                        "revision_id": "back-revision",
                        "basis": "latest_observed_no_errata",
                    },
                )
            for record in inputs.registry.records:
                db.insert(
                    "glossary_term",
                    {
                        "id": record.term_id,
                        "category": record.term_category,
                        "source_ja": _SPELLINGS.get(record.code, "クイック"),
                        "concept_key": record.term_id[5:],
                        "emphasis": True,
                        "authored_source_id": "source",
                        "record_key": record.term_id,
                        "origin": "project",
                        "low_confidence": False,
                    },
                )
                db.insert(
                    "glossary_translation",
                    {
                        "term_id": record.term_id,
                        "lang": "zh-Hant",
                        "text": "測試" + record.code,
                        "source_id": None,
                        "authored_source_id": "source",
                        "record_key": record.term_id + ":zh-Hant",
                        "origin": "project",
                        "low_confidence": True,
                    },
                )
        yield db, inputs, fixture.card.id


def printing(db: Database, cid: str, region: str) -> None:
    card = db.select("card", db.columns("card"), where={"id": cid})[0].values
    db.insert(
        "printing",
        {
            "id": region + "-printing",
            "card_id": cid,
            "region": region,
            "card_no": region + "-001",
            "card_no_state": "official",
            "catalog_state": "official",
            "decklog_available": True,
            "decklog_verification": "unverified",
            "variant_key": "default",
            "home_set_id": card["home_set_id"],
            "rarity_raw": "-",
            "source_id": "source",
        },
    )


def program(db: Database, cid: str, *abilities: dict[str, JsonValue]) -> None:
    """Admit one synthetic whole-card program the way program population stores it."""
    ast: dict[str, JsonValue] = {
        "card_id": cid,
        "scope": "shared",
        "abilities": [
            ability | {"line": line, "ports": _PORTS}
            for line, ability in enumerate(abilities, 1)
        ],
    }
    hashed = digest(canonical(ast))
    db.insert(
        "dsl_document",
        {
            "id": "dsl:" + hashed[7:],
            "card_id": cid,
            "scope": "shared",
            "dsl_version": "1.0",
            "status": "draft",
            "ast": Json(ast),
            "program_hash": hashed,
        },
    )


def project(db: Database, inputs: mechanics.Inputs) -> dict[str, JsonValue]:
    mechanics.register(db, inputs, REVISION)
    return mechanics.apply(db, inputs, REVISION)


def relations(db: Database) -> dict[str, list[JsonValue]]:
    found: dict[str, list[JsonValue]] = {}
    for row in db.rows("mechanic_projection"):
        value = row.values["relations"]
        assert isinstance(value, Json)
        found[str(row.values["keyword_id"])] = array(value.value)
    return found


_WARD_FOLLOWERS: dict[str, JsonValue] = {
    "keyword": "ward",
    "kind": "follower",
    "who": "self",
    "zone": "field",
}


def test_program_supplies_all_relations_and_proves_the_rest_absent(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    with case(tmp_path, monkeypatch, "Synthetic front.", back="") as (db, inputs, cid):
        with db.transaction():
            printing(db, cid, "jp")
            program(
                db,
                cid,
                {"keywords": ["ward"]},
                {
                    "fanfare": {
                        "do": [{"grant": {"keywords": ["rush"], "on": "ally"}}],
                        "targets": [
                            {
                                "bind": "ally",
                                "from": {
                                    "kind": "follower",
                                    "other": True,
                                    "who": "self",
                                    "zone": "field",
                                },
                                "n": 1,
                            }
                        ],
                    }
                },
                {
                    "evolve_time": {
                        "do": [
                            {
                                "buff": {
                                    "on": "this",
                                    "power": {"count": _WARD_FOLLOWERS},
                                }
                            },
                            {"buff": {"hp": 1, "on": _WARD_FOLLOWERS, "power": 1}},
                        ]
                    }
                },
            )
            report = project(db, inputs)
        assert relations(db) == {
            "kw:ward": ["counts", "has", "refers"],
            "kw:rush": ["grants"],
        }
        assert all(
            row.values["source_kind"] == "dsl" and row.values["dsl_id"] is not None
            for row in db.rows("card_mechanic")
        )
        coverage = db.rows("card_mechanic_coverage")[0].values
        # Only the concept outside program derivation stays unchecked.
        assert coverage["complete_all"] is False
        assert coverage["complete_mode"] == "exclude"
        assert coverage["complete_keyword_ids"] == Json(["kw:quick"])
        assert coverage["partial_keyword_ids"] == Json([])
        assert object_value(report["dsl"])["relations"] == {
            "counts": 1,
            "grants": 1,
            "has": 1,
            "refers": 1,
        }


@pytest.mark.parametrize(
    ("raw", "checked"),
    [("", True), ("Synthetic unauthored rule.", False)],
    ids=["blank", "unauthored"],
)
def test_only_a_blank_card_is_checked_without_a_program(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, raw: str, *, checked: bool
) -> None:
    with case(tmp_path, monkeypatch, raw) as (db, inputs, cid):
        with db.transaction():
            printing(db, cid, "jp")
            report = project(db, inputs)
        assert not db.rows("card_mechanic")
        assert bool(db.rows("mechanic_coverage")) is checked
        assert object_value(report["dsl"])["blank_cards"] == int(checked)
        coverage = db.rows("card_mechanic_coverage")[0].values
        assert coverage["complete_keyword_ids"] == Json(["kw:quick"] if checked else [])
        assert coverage["complete_mode"] == ("exclude" if checked else "include")


@pytest.mark.parametrize("gap", ["missing-face", "provisional"])
def test_missing_face_or_unconfirmed_identity_never_completes(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, gap: str
) -> None:
    with case(tmp_path, monkeypatch, "Synthetic front.", back="Synthetic back.") as (
        db,
        inputs,
        cid,
    ):
        with db.transaction():
            printing(db, cid, "jp")
            program(db, cid, {"keywords": ["ward"]})
            if gap == "provisional":
                db.update("card", {"id": cid}, {"identity_state": "provisional"})
            else:
                back = db.select(
                    "face_current", ("face_id",), where={"revision_id": "back-revision"}
                )[0].values["face_id"]
                db.delete("face_current", {"face_id": back, "region": "jp"})
            project(db, inputs)
        coverage = db.rows("card_mechanic_coverage")[0].values
        assert coverage["complete_all"] is False
        assert coverage["complete_keyword_ids"] == Json([])
        # A known label stays usable; the unseen face only blocks absence.
        assert relations(db) == ({} if gap == "provisional" else {"kw:ward": ["has"]})


def clear_projection(db: Database) -> None:
    for table in ("mechanic_projection", "card_mechanic_coverage"):
        for row in db.rows(table):
            keys = (
                ("card_id", "keyword_id", "scope")
                if table == "mechanic_projection"
                else ("card_id", "scope")
            )
            db.delete(table, {key: row.values[key] for key in keys})


@pytest.mark.parametrize("stale", ["source", "producer", "scope"])
def test_stale_or_wrong_scope_evidence_does_not_become_public(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    stale: str,
) -> None:
    with case(tmp_path, monkeypatch, "Synthetic rule.") as (db, inputs, cid):
        with db.transaction():
            printing(db, cid, "jp")
            program(db, cid, {"keywords": ["ward"]})
            project(db, inputs)
        hashes = mechanics.source_bundles(db)
        tag = db.rows("card_mechanic")[0].values
        version = str(tag["producer_version"])
        with db.transaction():
            clear_projection(db)
            if stale == "source":
                hashes[cid] = "sha256:" + "f" * 64
            elif stale == "producer":
                version = "changed-producer"
            else:
                db.update("card_mechanic", {"id": tag["id"]}, {"scope": "en_override"})
                for row in db.rows("mechanic_coverage"):
                    db.update(
                        "mechanic_coverage",
                        {
                            key: row.values[key]
                            for key in (
                                "card_id",
                                "keyword_id",
                                "scope",
                                "producer_version",
                            )
                        },
                        {"scope": "en_override"},
                    )
            mechanic_coverage.project_current(db, hashes, frozenset({version}))
        assert not db.rows("mechanic_projection")
        assert not db.rows("card_mechanic_coverage")


def test_authored_exact_suppression_and_stale_suppression_are_imported_separately(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    with case(tmp_path, monkeypatch, "Synthetic rule.") as (db, inputs, cid):
        tags: list[dict[str, Value]] = []

        def preview() -> None:
            with db.transaction():
                printing(db, cid, "jp")
                program(db, cid, {"keywords": ["ward"]})
                project(db, inputs)
                tags.append(dict(db.rows("card_mechanic")[0].values))
                raise RuntimeError("rollback candidate preview")

        with pytest.raises(RuntimeError, match="rollback candidate preview"):
            preview()
        tag = tags[0]
        exact = mechanics.Suppression(
            id="override:exact",
            target_tag_id=str(tag["id"]),
            source_text_hash=str(tag["source_text_hash"]),
            producer_version=str(tag["producer_version"]),
            reason="Synthetic correction.",
            decision=mechanics.Adoption(
                id="decision:correction",
                state="confirmed",
                note="Synthetic exact correction.",
            ),
        )
        stale = exact.model_copy(
            update={
                "id": "override:stale",
                "target_tag_id": "mechanic:retired",
                "decision": exact.decision.model_copy(update={"id": "decision:stale"}),
            }
        )
        changed = replace(
            inputs,
            registry=inputs.registry.model_copy(
                update={"suppressions": (exact, stale)}
            ),
        )
        with db.transaction():
            printing(db, cid, "jp")
            program(db, cid, {"keywords": ["ward"]})
            report = project(db, changed)
        assert object_value(report["reasons"])["stale_suppression"] == 1
        assert len(db.rows("mechanic_override")) == 1
        decisions = {str(row.values["id"]): row.values for row in db.rows("decision")}
        assert decisions["mapping:synthetic"]["category"] == "keyword_mapping"
        assert decisions["decision:correction"]["category"] == "mechanic_suppression"
        assert "decision:stale" not in decisions
        assert all(
            row.values["decision_id"] != "decision:stale"
            for row in db.rows("decision_source")
        )
        # Suppressing the label never reopens the checked fields.
        assert not db.rows("mechanic_projection")
        coverage = db.rows("card_mechanic_coverage")[0].values
        assert coverage["complete_keyword_ids"] == Json(["kw:quick"])


def inspection(
    tag: Mapping[str, Value],
    version: str,
    checks: tuple[mechanic_coverage.FieldCheck, ...],
) -> mechanic_coverage.Inspection:
    return mechanic_coverage.Inspection(
        card_id=str(tag["card_id"]),
        keyword_id="kw:quick",
        scope="shared",
        source_kind="manual",
        source_text_hash=str(tag["source_text_hash"]),
        producer_version=version,
        decision_id=None,
        dsl_id=None,
        checks=checks,
    )


def test_reducer_requires_every_relation_on_every_face_and_section(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    with case(
        tmp_path,
        monkeypatch,
        "Synthetic rule.",
        back="Synthetic back effect.",
        sections=("Synthetic section.",),
    ) as (db, inputs, cid):
        with db.transaction():
            printing(db, cid, "jp")
            program(db, cid, {"keywords": ["ward"]})
            project(db, inputs)
            tag = db.rows("card_mechanic")[0].values
            dsl = str(tag["producer_version"])
            fields: list[tuple[str, str, Literal["effect", "section"], int | None]] = [
                (
                    str(row.values["face_id"]),
                    str(row.values["revision_id"]),
                    "effect",
                    None,
                )
                for row in db.rows("face_current")
                if row.values["region"] == "jp"
            ]
            front = next(
                face for face, revision, _, _ in fields if revision == "link-revision"
            )
            fields.append((front, "link-revision", "section", 0))
            checks = tuple(
                mechanic_coverage.FieldCheck(
                    face_id=face,
                    revision_id=revision,
                    field=field,
                    ordinal=ordinal,
                    relation=relation,
                    whole_field=True,
                )
                for face, revision, field, ordinal in fields
                for relation in ("has", "grants", "refers")
            )
            mechanic_coverage.store_evidence(
                db, (), (inspection(tag, "three-relations", checks),)
            )
            clear_projection(db)
            mechanic_coverage.project_current(
                db, mechanics.source_bundles(db), frozenset({dsl, "three-relations"})
            )
        coverage = db.rows("card_mechanic_coverage")[0].values
        assert coverage["complete_keyword_ids"] == Json(["kw:quick"])
        assert coverage["complete_mode"] == "exclude"
        assert coverage["partial_keyword_ids"] == Json([])
        assert coverage["partial_mode"] == "exclude"
        counts = tuple(
            check.model_copy(update={"relation": "counts"})
            for check in checks
            if check.relation == "has"
        )
        with db.transaction():
            mechanic_coverage.store_evidence(
                db, (), (inspection(tag, "counts-main", counts[:-1]),)
            )
            clear_projection(db)
            mechanic_coverage.project_current(
                db,
                mechanics.source_bundles(db),
                frozenset({dsl, "three-relations", "counts-main"}),
            )
        assert db.rows("card_mechanic_coverage")[0].values["complete_all"] is False
        with db.transaction():
            mechanic_coverage.store_evidence(
                db, (), (inspection(tag, "counts-section", counts[-1:]),)
            )
            clear_projection(db)
            mechanic_coverage.project_current(
                db,
                mechanics.source_bundles(db),
                frozenset({dsl, "three-relations", "counts-main", "counts-section"}),
            )
        # Two producers' checks merge; only all four relations on every field complete.
        assert db.rows("card_mechanic_coverage")[0].values["complete_all"] is True


def test_changing_only_the_other_face_invalidates_the_card_bundle(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    with case(tmp_path, monkeypatch, "Synthetic rule.", back="Synthetic back.") as (
        db,
        _,
        cid,
    ):
        before = mechanics.source_bundles(db)[cid]
        with db.transaction():
            unit = TextInterner(db).intern(
                LocalizedText(lang="ja", text="Changed synthetic back effect.")
            )
            db.update(
                "face_revision", {"id": "back-revision"}, {"effect_unit_id": unit}
            )
        assert mechanics.source_bundles(db)[cid] != before


def test_registry_rejects_duplicate_concepts_wrong_ids_and_unconfirmed_decisions() -> (
    None
):
    raw = registry().model_dump(mode="json")
    record = object_value(array(raw["records"])[0])
    for changed in (
        raw | {"records": [record, record]},
        raw | {"records": [record | {"id": "kw:wrong"}]},
        raw | {"format": True},
        raw
        | {"decision": {"id": "pending", "state": "proposed", "note": "Synthetic."}},
    ):
        with pytest.raises(ValidationError):
            mechanics.Registry.model_validate_json(canonical(changed))


def test_loader_is_bounded_and_uses_explicit_registry(
    tmp_path: Path,
) -> None:
    root = tmp_path / "authored"
    path = root / mechanics.FILE
    path.parent.mkdir(parents=True)
    path.write_bytes(canonical(registry().model_dump(mode="json")))
    assert mechanics.load(root).registry == registry()
    path.write_text("format: true\nkind: keyword_registry\n")
    with pytest.raises(ValueError, match="Invalid keyword registry"):
        mechanics.load(root)
    path.unlink()
    path.symlink_to(tmp_path / "unrelated")
    with pytest.raises(ValueError, match="Symlink"):
        mechanics.load(root)


def test_wrong_glossary_category_is_rejected_before_projection(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    with case(tmp_path, monkeypatch, "Synthetic rule.") as (db, inputs, _):
        with db.transaction():
            db.update(
                "glossary_term", {"id": "term:keyword.ward"}, {"category": "ability"}
            )
        with (
            pytest.raises(ValueError, match="existing keyword concept"),
            db.transaction(),
        ):
            mechanics.register(db, inputs, REVISION)
        assert not db.rows("keyword")


@pytest.mark.parametrize("official", [True, False])
def test_keyword_labels_preserve_glossary_authority_source_and_term_quality(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    official: bool,
) -> None:
    with case(tmp_path, monkeypatch, "Synthetic rule.") as (db, inputs, _):
        with db.transaction():
            db.insert(
                "source_record",
                {
                    "id": "dictionary:synthetic",
                    "kind": "official_api",
                    "url": "https://example.invalid/synthetic-dictionary",
                    "fetched_at": "2026-10-01T00:00:00Z",
                    "sha256": "sha256:" + "c" * 64,
                },
            )
            db.update(
                "glossary_term", {"id": "term:keyword.ward"}, {"low_confidence": True}
            )
            db.update(
                "glossary_translation",
                {"term_id": "term:keyword.ward", "lang": "zh-Hant"},
                {
                    "origin": "official" if official else "machine",
                    "source_id": "dictionary:synthetic",
                    "low_confidence": False,
                },
            )
            mechanics.register(db, inputs, REVISION)
        chosen = next(
            row.values
            for row in db.rows("translation")
            if row.values["text"] == "測試ward"
        )
        assert chosen["origin"] == ("official" if official else "machine")
        assert chosen["authority"] == ("digital_official" if official else "unofficial")
        assert chosen["source_id"] == "dictionary:synthetic"
        assert chosen["low_confidence"] is True


@pytest.mark.parametrize("mutation", ["authority", "source", "field", "borrowed_owner"])
def test_official_keyword_selection_cannot_leak_to_other_owners_or_authorities(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    mutation: str,
) -> None:
    with case(tmp_path, monkeypatch, "Synthetic rule.") as (db, inputs, _):
        with db.transaction():
            mechanics.register(db, inputs, REVISION)
            use = db.rows("translation_use")[0].values
            chosen = next(
                row.values
                for row in db.rows("translation")
                if row.values["context_id"] == use["context_id"]
            )
            db.update(
                "translation",
                {"id": chosen["id"]},
                {
                    "origin": "official",
                    "authority": "digital_official",
                    "source_id": "source",
                },
            )

        def invalidate_selection() -> None:
            with db.transaction():
                if mutation == "borrowed_owner":
                    db.insert(
                        "translation_use",
                        dict(use)
                        | {
                            "id": "use:borrowed",
                            "keyword_id": None,
                            "face_revision_id": "link-revision",
                            "field": "name",
                        },
                    )
                elif mutation == "field":
                    db.update(
                        "translation_use", {"id": use["id"]}, {"field": "action_label"}
                    )
                else:
                    db.update(
                        "translation",
                        {"id": chosen["id"]},
                        {"source_id": None}
                        if mutation == "source"
                        else {"authority": "sve_official"},
                    )

        with pytest.raises(
            sqlite3.IntegrityError, match="name_selection_owner_eligibility"
        ):
            invalidate_selection()
