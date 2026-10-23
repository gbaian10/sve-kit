"""Mechanism facts require exact frozen ownership, not mentions or successful rendering."""

import sqlite3
from contextlib import contextmanager
from dataclasses import replace
from typing import TYPE_CHECKING, Literal

import pytest
from pydantic import ValidationError

from sve_carddb.build import Json, create_database
from sve_carddb.build.t1 import compile_build
from sve_carddb.core.json import array, canonical, object_value
from sve_carddb.domains import mechanic_coverage, mechanic_grants, mechanics
from sve_carddb.domains.products.models import LocalizedText
from sve_carddb.domains.registry.records import FaceData
from sve_carddb.domains.registry.snapshot import load_registry
from sve_carddb.domains.text_observations.intern import TextInterner
from sve_carddb.domains.translations.four_layer_classification import Term

from ..support import digital_link_import_fixtures
from ..support.digital_link_import_fixtures import make_fixture
from ..support.translation_fixtures import template
from .translations.test_four_layer_classification import classifier

if TYPE_CHECKING:
    from collections.abc import Generator, Mapping
    from pathlib import Path

    from sve_carddb.build import Database, Value


def registry() -> mechanics.Registry:
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
                "records": [
                    {
                        "id": "kw:test-a",
                        "code": "test-a",
                        "term_id": "term:keyword.test_a",
                    },
                    {
                        "id": "kw:test-b",
                        "code": "test-b",
                        "term_id": "term:keyword.test_b",
                    },
                ],
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
    first_batch: bool = False,
) -> Generator[tuple[Database, mechanics.Inputs, mechanics.Settings, str]]:
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
        html.replace(b"Synthetic rule.", raw.replace("\n", "<br>").encode()),
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
    if first_batch:
        adopted = inputs.registry.model_copy(
            update={
                "records": tuple(
                    record.model_copy(update={"id": "kw:" + code, "code": code})
                    for record, code in zip(
                        inputs.registry.records, ("ward", "storm"), strict=True
                    )
                )
            }
        )
        inputs = mechanics.Inputs(adopted, canonical(adopted.model_dump(mode="json")))
    engine = classifier(
        (
            Term("term:keyword.test_a", "keyword", "合成甲"),
            Term("term:keyword.test_b", "keyword", "合成乙"),
        ),
        extra=("bracket_keyword_reference",),
    )
    schema = compile_build(("keyword", "correction"))
    with create_database(schema) as db, template().copy() as baseline:
        with db.transaction():
            for table in compile_build(("t0", "translation_evidence")).tables:
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
            for term in engine.terms.values():
                db.insert(
                    "glossary_term",
                    {
                        "id": term.id,
                        "category": term.category,
                        "source_ja": term.source_ja,
                        "concept_key": term.id[5:],
                        "emphasis": True,
                        "authored_source_id": "source",
                        "record_key": term.id,
                        "origin": "project",
                        "low_confidence": False,
                    },
                )
                db.insert(
                    "glossary_translation",
                    {
                        "term_id": term.id,
                        "lang": "zh-Hant",
                        "text": "測試" + term.source_ja,
                        "source_id": None,
                        "authored_source_id": "source",
                        "record_key": term.id + ":zh-Hant",
                        "origin": "project",
                        "low_confidence": True,
                    },
                )
        settings = mechanics.Settings(
            fixture.sources(), engine, fixture.jp.batch_id, fixture.authored
        )
        if first_batch:
            settings = replace(settings, first_batch_cards=frozenset({fixture.card.id}))
        yield db, inputs, settings, fixture.card.id


def test_first_batch_declarations_and_empty_back_prove_all_keywords(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    with case(
        tmp_path, monkeypatch, "【合成甲】【合成乙】", back="", first_batch=True
    ) as (db, inputs, settings, cid):
        support = db.rows("card_engine_support")
        with db.transaction():
            report = mechanics.apply(db, inputs, settings)
        coverage = db.rows("card_mechanic_coverage")
        assert len(coverage) == 1
        assert coverage[0].values == {
            "card_id": cid,
            "scope": "shared",
            "complete_all": True,
            "complete_mode": "include",
            "complete_keyword_ids": Json([]),
            "partial_mode": "include",
            "partial_keyword_ids": Json([]),
        }
        assert {row.values["keyword_id"] for row in db.rows("mechanic_projection")} == {
            "kw:ward",
            "kw:storm",
        }
        assert all(
            row.values["relations"] == Json(["has"])
            for row in db.rows("mechanic_projection")
        )
        assert report["complete_cards"] == 1
        assert db.rows("card_engine_support") == support


def test_first_batch_missing_current_back_prevents_complete(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    with case(
        tmp_path, monkeypatch, "【合成甲】", back="【合成乙】", first_batch=True
    ) as (db, inputs, settings, _):
        with db.transaction():
            back = db.select(
                "face_current",
                db.columns("face_current"),
                where={"revision_id": "back-revision"},
            )[0].values
            db.delete("face_current", {"face_id": back["face_id"], "region": "jp"})
            report = mechanics.apply(db, inputs, settings)
        coverage = db.rows("card_mechanic_coverage")[0].values
        assert coverage["complete_all"] is False
        assert coverage["complete_keyword_ids"] == Json([])
        assert coverage["partial_mode"] == "exclude"
        assert coverage["partial_keyword_ids"] == Json([])
        assert report["complete_cards"] == 0
        projection = db.rows("mechanic_projection")
        assert len(projection) == 1
        assert projection[0].values["keyword_id"] == "kw:ward"


def test_other_card_grant_merges_with_self_declaration_but_stays_partial(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    raw = "【合成甲】\n{ファンファーレ}自分の他のフォロワー1体を選ぶ。それは【合成甲】を持つ。"
    with case(tmp_path, monkeypatch, raw, first_batch=True) as (
        db,
        inputs,
        settings,
        cid,
    ):
        with db.transaction():
            mechanics.apply(db, inputs, settings)
        projection = db.rows("mechanic_projection")
        assert len(projection) == 1
        assert projection[0].values["card_id"] == cid
        assert projection[0].values["relations"] == Json(["grants", "has"])
        evidence = db.rows("card_mechanic")
        assert len({row.values["producer_version"] for row in evidence}) == 2
        coverage = db.rows("card_mechanic_coverage")[0].values
        assert coverage["complete_all"] is False
        assert coverage["complete_keyword_ids"] == Json([])
        assert coverage["partial_keyword_ids"] == Json(["kw:ward"])
        with db.transaction():
            clear_projection(db)
            db.update(
                "face_revision",
                {"id": "link-revision"},
                {
                    "effect_unit_id": TextInterner(db).intern(
                        LocalizedText(lang="ja", text=raw + "\nBorrowed source.")
                    )
                },
            )
        with pytest.raises(ValueError, match="frozen field differs"), db.transaction():
            mechanic_grants.apply(db, inputs, settings, mechanics.source_bundles(db))


def test_keyword_selection_and_threshold_merge_without_completing_coverage(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    raw = (
        "【合成甲】\n"
        "{ファンファーレ}自分の他のフォロワー1体を選ぶ。それは【合成甲】を持つ。\n"
        "{起動}{コスト2}これを{アクト}墓場に置く："
        "【合成甲】を持つ自分のフォロワー1体を選ぶ。それは{攻撃力}+2/{体力}+2する。\n"
        "{起動}{コスト1}手札の【合成甲】を持つフォロワー1枚を捨てる：1枚引く。\n"
        "{ファンファーレ}自分の場の【合成甲】を持つフォロワーが2体以上なら、1枚引く。"
    )
    with case(tmp_path, monkeypatch, raw, first_batch=True) as (
        db,
        inputs,
        settings,
        cid,
    ):
        with db.transaction():
            mechanics.apply(db, inputs, settings)
        row = db.rows("mechanic_projection")[0].values
        assert row["card_id"] == cid
        assert row["relations"] == Json(["counts", "grants", "has", "refers"])
        assert row["actions"] == Json([])
        coverage = db.rows("card_mechanic_coverage")[0].values
        assert coverage["complete_all"] is False
        assert coverage["complete_keyword_ids"] == Json([])
        assert coverage["partial_keyword_ids"] == Json(["kw:ward"])


def test_quoted_grants_and_token_sections_do_not_grant_the_parent_card(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    grant = "自分の他のフォロワー1体を選ぶ。それは【合成甲】を持つ。"
    reference = (
        "{起動}{コスト2}これを{アクト}墓場に置く："
        "【合成甲】を持つ自分のフォロワー1体を選ぶ。それは{攻撃力}+2/{体力}+2する。"
    )
    count = (
        "{ファンファーレ}自分の場の【合成甲】を持つフォロワーが2体以上なら、1枚引く。"
    )
    raw = "これは「\n" + grant + "\n" + reference + "\n" + count + "\n」を持つ。"
    with case(tmp_path, monkeypatch, raw, first_batch=True) as (
        db,
        inputs,
        settings,
        _,
    ):
        with db.transaction():
            db.insert(
                "face_text_section",
                {
                    "revision_id": "link-revision",
                    "ordinal": 0,
                    "kind": "token_definition",
                    "text_unit_id": TextInterner(db).intern(
                        LocalizedText(
                            lang="ja",
                            text="『合成トークン』"
                            + grant
                            + "\n"
                            + reference
                            + "\n"
                            + count,
                        )
                    ),
                },
            )
            mechanics.apply(db, inputs, settings)
        assert not db.rows("mechanic_projection")
        assert not db.rows("card_mechanic_coverage")


@pytest.mark.parametrize(
    ("text", "expected"),
    [
        ("【合成甲】", {"kw:test-a"}),
        ("【合成甲】【合成乙】", {"kw:test-a", "kw:test-b"}),
        ("【合成甲】\nSynthetic undecoded effect.", {"kw:test-a"}),
        ("それは【合成甲】を持つ。", set()),
        ("これは【合成甲】を失う。", set()),
        ("【合成甲】を持つフォロワーを選ぶ。", set()),
        ("「【合成甲】」を持つ。", set()),
        ("【未知合成】", set()),
        ("Synthetic undecoded effect.", set()),
    ],
)
def test_only_whole_declarations_are_positive_without_render_targets(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    text: str,
    expected: set[str],
) -> None:
    with case(tmp_path, monkeypatch, text) as (db, inputs, settings, cid):
        support = db.rows("card_engine_support")
        with db.transaction():
            report = mechanics.apply(db, inputs, settings)
        rows = db.rows("mechanic_projection")
        assert {row.values["keyword_id"] for row in rows} == expected
        assert all(
            row.values["relations"] == Json(["has"])
            and row.values["actions"] == Json([])
            for row in rows
        )
        assert all(
            row.values["card_id"] == cid and row.values["scope"] == "shared"
            for row in rows
        )
        coverage = db.rows("card_mechanic_coverage")
        assert len(coverage) == bool(expected)
        assert all(
            row.values["complete_all"] is False
            and row.values["complete_keyword_ids"] == Json([])
            for row in coverage
        )
        assert report["complete_cards"] == 0
        assert db.rows("card_engine_support") == support
        translations = [row.values for row in db.rows("translation")]
        assert all(row["low_confidence"] is True for row in translations)
        assert {row["text"] for row in translations} == {"測試合成甲", "測試合成乙"}
        assert all(
            row.values["source_kind"] == "manual" and row.values["dsl_id"] is None
            for row in db.rows("card_mechanic")
        )


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
    with case(tmp_path, monkeypatch, "【合成甲】") as (db, inputs, settings, cid):
        with db.transaction():
            mechanics.apply(db, inputs, settings)
        hashes = mechanics.source_bundles(db)
        version = mechanics.producer_version(inputs, settings.classifier)
        with db.transaction():
            clear_projection(db)
            if stale == "source":
                hashes[cid] = "sha256:" + "f" * 64
            elif stale == "producer":
                version = "changed-producer"
            else:
                tag = db.rows("card_mechanic")[0].values
                db.update("card_mechanic", {"id": tag["id"]}, {"scope": "en_override"})
                db.update(
                    "mechanic_coverage",
                    {
                        "card_id": cid,
                        "keyword_id": "kw:test-a",
                        "scope": "shared",
                        "producer_version": mechanics.producer_version(
                            inputs, settings.classifier
                        ),
                    },
                    {"scope": "en_override"},
                )
            mechanic_coverage.project_current(db, hashes, frozenset({version}))
        assert not db.rows("mechanic_projection")
        assert not db.rows("card_mechanic_coverage")


@pytest.mark.parametrize("fresh", [True, False])
def test_suppression_is_exact_and_does_not_clear_partial_coverage(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    fresh: bool,
) -> None:
    with case(tmp_path, monkeypatch, "【合成甲】") as (db, inputs, settings, _):
        with db.transaction():
            mechanics.apply(db, inputs, settings)
        tag = db.rows("card_mechanic")[0].values
        with db.transaction():
            clear_projection(db)
            db.insert(
                "mechanic_override",
                {
                    "id": "override:synthetic",
                    "target_tag_id": tag["id"],
                    "operation": "suppress",
                    "source_text_hash": tag["source_text_hash"]
                    if fresh
                    else "sha256:" + "f" * 64,
                    "producer_version": tag["producer_version"],
                    "reason": "Synthetic correction.",
                    "decision_id": "mapping:synthetic",
                },
            )
            mechanic_coverage.project_current(
                db,
                mechanics.source_bundles(db),
                frozenset({mechanics.producer_version(inputs, settings.classifier)}),
            )
        assert bool(db.rows("mechanic_projection")) is not fresh
        assert len(db.rows("card_mechanic_coverage")) == 1
        assert db.rows("card_mechanic_coverage")[0].values["complete_all"] is False


def test_duplicate_positive_evidence_deduplicates_and_stays_partial(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    with case(tmp_path, monkeypatch, "【合成甲】", first_batch=True) as (
        db,
        inputs,
        settings,
        cid,
    ):
        before = mechanics.source_bundles(db)[cid]
        with db.transaction():
            db.insert(
                "face_text_section",
                {
                    "revision_id": "link-revision",
                    "ordinal": 0,
                    "text_unit_id": db.rows("face_revision")[-1].values[
                        "effect_unit_id"
                    ],
                    "kind": "token_definition",
                },
            )
        assert mechanics.source_bundles(db)[cid] != before
        with db.transaction():
            mechanics.apply(db, inputs, settings)
            assert db.rows("card_mechanic_coverage")[0].values["complete_all"] is False
            clear_projection(db)
            tag = db.rows("card_mechanic")[0].values
            db.insert(
                "card_mechanic", dict(tag) | {"id": "mechanic:second-synthetic-source"}
            )
            mechanic_coverage.project_current(
                db,
                mechanics.source_bundles(db),
                frozenset({mechanics.producer_version(inputs, settings.classifier)}),
            )
        assert len(db.rows("mechanic_projection")) == 1
        assert db.rows("card_mechanic_coverage")[0].values["complete_all"] is False


def test_keyword_only_in_token_section_does_not_mark_the_original_card(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    with case(tmp_path, monkeypatch, "Synthetic undecoded effect.") as (
        db,
        inputs,
        settings,
        _,
    ):
        with db.transaction():
            unit = TextInterner(db).intern(LocalizedText(lang="ja", text="【合成甲】"))
            db.insert(
                "face_text_section",
                {
                    "revision_id": "link-revision",
                    "ordinal": 0,
                    "text_unit_id": unit,
                    "kind": "token_definition",
                },
            )
            mechanics.apply(db, inputs, settings)
        assert not db.rows("card_mechanic")
        assert not db.rows("mechanic_projection")
        assert not db.rows("card_mechanic_coverage")


def test_only_the_back_face_declaration_marks_the_card(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    with case(tmp_path, monkeypatch, "Synthetic front effect.", back="【合成甲】") as (
        db,
        inputs,
        settings,
        cid,
    ):
        with db.transaction():
            mechanics.apply(db, inputs, settings)
        projection = db.rows("mechanic_projection")
        assert len(projection) == 1
        assert projection[0].values["card_id"] == cid
        assert projection[0].values["keyword_id"] == "kw:test-a"
        assert projection[0].values["relations"] == Json(["has"])


def test_changing_only_the_other_face_invalidates_the_card_bundle(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    with case(tmp_path, monkeypatch, "【合成甲】", back="Synthetic back effect.") as (
        db,
        inputs,
        settings,
        cid,
    ):
        with db.transaction():
            mechanics.apply(db, inputs, settings)
        before = mechanics.source_bundles(db)[cid]
        with db.transaction():
            unit = TextInterner(db).intern(
                LocalizedText(lang="ja", text="Changed synthetic back effect.")
            )
            db.update(
                "face_revision", {"id": "back-revision"}, {"effect_unit_id": unit}
            )
        assert mechanics.source_bundles(db)[cid] != before


def test_borrowed_source_text_fails_even_when_it_is_a_keyword(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    with case(tmp_path, monkeypatch, "【合成甲】") as (db, inputs, settings, _):
        with pytest.raises(ValueError, match="frozen field differs"), db.transaction():  # ruff: ignore[pytest-raises-with-multiple-statements] -- the deliberately invalid source and its producer run must roll back together
            unit = TextInterner(db).intern(LocalizedText(lang="ja", text="【合成乙】"))
            db.update(
                "face_revision", {"id": "link-revision"}, {"effect_unit_id": unit}
            )
            mechanics.apply(db, inputs, settings)
        assert not db.rows("mechanic_projection")


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


@pytest.mark.parametrize("mode", ["unregistered", "unconfirmed", "empty"])
def test_unusable_sources_never_become_absent_or_implicit_keywords(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    mode: str,
) -> None:
    raw = "" if mode == "empty" else "【合成甲】"
    with case(tmp_path, monkeypatch, raw) as (db, inputs, settings, cid):
        selected = inputs
        if mode == "unregistered":
            selected = replace(
                inputs,
                registry=inputs.registry.model_copy(
                    update={"records": inputs.registry.records[1:]}
                ),
            )
        with db.transaction():
            if mode == "unconfirmed":
                db.update("card", {"id": cid}, {"identity_state": "provisional"})
            report = mechanics.apply(db, selected, settings)
        assert not db.rows("mechanic_projection")
        assert not db.rows("card_mechanic_coverage")
        assert report["reasons"]


def test_wrong_glossary_category_is_rejected_before_projection(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    with case(tmp_path, monkeypatch, "【合成甲】") as (db, inputs, settings, _):
        with db.transaction():
            db.update(
                "glossary_term", {"id": "term:keyword.test_a"}, {"category": "ability"}
            )
        with (
            pytest.raises(ValueError, match="existing keyword concept"),
            db.transaction(),
        ):
            mechanics.apply(db, inputs, settings)
        assert not db.rows("keyword")


@pytest.mark.parametrize("official", [True, False])
def test_keyword_labels_preserve_glossary_authority_source_and_term_quality(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    official: bool,
) -> None:
    with case(tmp_path, monkeypatch, "【合成甲】") as (db, inputs, settings, _):
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
                "glossary_term", {"id": "term:keyword.test_a"}, {"low_confidence": True}
            )
            db.update(
                "glossary_translation",
                {"term_id": "term:keyword.test_a", "lang": "zh-Hant"},
                {
                    "origin": "official" if official else "machine",
                    "source_id": "dictionary:synthetic",
                    "low_confidence": False,
                },
            )
            mechanics.apply(db, inputs, settings)
        chosen = next(
            row.values
            for row in db.rows("translation")
            if row.values["text"] == "測試合成甲"
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
    with case(tmp_path, monkeypatch, "【合成甲】") as (db, inputs, settings, _):
        with db.transaction():
            mechanics.apply(db, inputs, settings)
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


def test_authored_exact_suppression_and_stale_suppression_are_imported_separately(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    with case(tmp_path, monkeypatch, "【合成甲】") as (db, inputs, settings, _):
        tags: list[dict[str, Value]] = []

        def preview() -> None:
            with db.transaction():
                mechanics.apply(db, inputs, settings)
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
            report = mechanics.apply(db, changed, settings)
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
        assert not db.rows("mechanic_projection")
        assert db.rows("card_mechanic_coverage")[0].values["complete_all"] is False


def test_h1_rejects_complete_coverage_and_uses_shortest_partial_encoding(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    with case(tmp_path, monkeypatch, "【合成甲】【合成乙】") as (
        db,
        inputs,
        settings,
        cid,
    ):
        with db.transaction():
            mechanics.apply(db, inputs, settings)
        row = db.rows("card_mechanic_coverage")[0].values
        assert row["partial_mode"] == "exclude"
        assert row["partial_keyword_ids"] == Json([])
        with db.transaction():
            db.update(
                "mechanic_coverage",
                {
                    "card_id": cid,
                    "keyword_id": "kw:test-a",
                    "scope": "shared",
                    "producer_version": mechanics.producer_version(
                        inputs, settings.classifier
                    ),
                },
                {"state": "complete"},
            )

        def reproject() -> None:
            with db.transaction():
                clear_projection(db)
                mechanic_coverage.project_current(
                    db,
                    mechanics.source_bundles(db),
                    frozenset(
                        {mechanics.producer_version(inputs, settings.classifier)}
                    ),
                )

        with pytest.raises(ValueError, match="state differs from checked ranges"):
            reproject()


def inspection(
    tag: Mapping[str, Value],
    version: str,
    checks: tuple[mechanic_coverage.FieldCheck, ...],
) -> mechanic_coverage.Inspection:
    return mechanic_coverage.Inspection(
        card_id=str(tag["card_id"]),
        keyword_id=str(tag["keyword_id"]),
        scope="shared",
        source_kind="manual",
        source_text_hash=str(tag["source_text_hash"]),
        producer_version=version,
        decision_id=str(tag["decision_id"]),
        dsl_id=None,
        checks=checks,
    )


def test_reducer_unions_two_producers_without_overwriting_checks(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    with case(tmp_path, monkeypatch, "【合成甲】") as (db, inputs, settings, _):
        with db.transaction():
            mechanics.apply(db, inputs, settings)
            tag = db.rows("card_mechanic")[0].values
            coverage = db.rows("mechanic_coverage")[0].values
            raw = coverage["checked_ranges"]
            assert isinstance(raw, Json)
            first = mechanic_coverage.FieldCheck.model_validate_json(
                canonical(array(raw.value)[0])
            )
            second = first.model_copy(update={"relation": "grants"})
            mechanic_coverage.store_evidence(
                db,
                (
                    dict(tag)
                    | {
                        "id": "mechanic:grants",
                        "relation": "grants",
                        "producer_version": "grants-v1",
                    },
                ),
                (inspection(tag, "grants-v1", (second,)),),
            )
            clear_projection(db)
            mechanic_coverage.project_current(
                db,
                mechanics.source_bundles(db),
                frozenset({str(tag["producer_version"]), "grants-v1"}),
            )
        assert len(db.rows("mechanic_coverage")) == 2
        assert db.rows("mechanic_projection")[0].values["relations"] == Json(
            ["grants", "has"]
        )
        assert db.rows("card_mechanic_coverage")[0].values["complete_all"] is False


def test_reducer_requires_every_relation_on_every_face_and_section(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    with case(tmp_path, monkeypatch, "【合成甲】", back="Synthetic back effect.") as (
        db,
        inputs,
        settings,
        _,
    ):
        with db.transaction():
            db.insert(
                "face_text_section",
                {
                    "revision_id": "link-revision",
                    "ordinal": 0,
                    "text_unit_id": db.rows("face_revision")[0].values[
                        "effect_unit_id"
                    ],
                    "kind": "token_definition",
                },
            )
            mechanics.apply(db, inputs, settings)
            tag = db.rows("card_mechanic")[0].values
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
                db, mechanics.source_bundles(db), frozenset({"three-relations"})
            )
        coverage = db.rows("card_mechanic_coverage")[0].values
        assert coverage["complete_keyword_ids"] == Json([])
        assert coverage["partial_keyword_ids"] == Json(["kw:test-a"])
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
                frozenset({"three-relations", "counts-main"}),
            )
        assert db.rows("card_mechanic_coverage")[0].values[
            "complete_keyword_ids"
        ] == Json([])
        with db.transaction():
            mechanic_coverage.store_evidence(
                db, (), (inspection(tag, "counts-section", counts[-1:]),)
            )
            clear_projection(db)
            mechanic_coverage.project_current(
                db,
                mechanics.source_bundles(db),
                frozenset({"three-relations", "counts-main", "counts-section"}),
            )
        coverage = db.rows("card_mechanic_coverage")[0].values
        assert coverage["complete_keyword_ids"] == Json(["kw:test-a"])
        assert coverage["complete_all"] is False
        assert coverage["partial_keyword_ids"] == Json([])
