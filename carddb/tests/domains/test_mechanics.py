"""Mechanism facts require exact frozen ownership, not mentions or successful rendering."""

import sqlite3
from contextlib import contextmanager
from dataclasses import replace
from typing import TYPE_CHECKING, Literal

import pytest
from pydantic import JsonValue, ValidationError

from sve_carddb.build import Json, create_database
from sve_carddb.build.t1 import compile_build
from sve_carddb.core.json import array, canonical, object_value
from sve_carddb.domains import mechanic_coverage, mechanic_grants, mechanics
from sve_carddb.domains.products.models import LocalizedText
from sve_carddb.domains.registry.records import FaceData
from sve_carddb.domains.registry.snapshot import load_registry
from sve_carddb.domains.text_observations.intern import TextInterner
from sve_carddb.domains.translations.four_layer_classification import Classifier, Term

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


def _case_keywords(first_batch: bool | str) -> tuple[mechanics.Inputs, Classifier]:
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
    if isinstance(first_batch, str):
        spellings = {
            "ward": "守護",
            "storm": "疾走",
            "rush": "突進",
            "bane": "必殺",
            "assail": "指定攻撃",
            "intimidate": "威圧",
            "aura": "オーラ",
            "drain": "ドレイン",
            "ambush": "潜伏",
            "barrier": "バリア",
        }
        adopted = inputs.registry.model_copy(
            update={
                "records": tuple(
                    mechanics.Keyword(
                        id="kw:" + code, code=code, term_id="term:keyword." + code
                    )
                    for code in spellings
                )
            }
        )
        inputs = mechanics.Inputs(adopted, canonical(adopted.model_dump(mode="json")))
        engine = classifier(
            tuple(
                Term("term:keyword." + code, "keyword", spelling)
                for code, spelling in spellings.items()
            ),
            extra=("bracket_keyword_reference",),
        )
    return inputs, engine


@contextmanager
def case(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    raw: str,
    *,
    back: str | None = None,
    first_batch: bool | str = False,
    sections: tuple[str, ...] = (),
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
    if isinstance(first_batch, str):
        faces = [face.model_copy(update={"card_id": first_batch}) for face in faces]
        fixture = replace(
            fixture, card=fixture.card.model_copy(update={"id": first_batch})
        )
    fixture = replace(fixture, face=faces[0])
    inputs, engine = _case_keywords(first_batch)
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


def test_first_batch_pp_evolution_and_declarations_prove_without_new_labels(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    with case(
        tmp_path,
        monkeypatch,
        "{進化}{コスト2}：これは進化する。\n【合成甲】",
        back="{進化}{コスト0}：これは進化する。",
        first_batch=True,
    ) as (db, inputs, settings, _):
        with db.transaction():
            report = mechanics.apply(db, inputs, settings)
        assert db.rows("card_mechanic_coverage")[0].values["complete_all"] is True
        projection = db.rows("mechanic_projection")
        assert len(projection) == 1
        assert projection[0].values["keyword_id"] == "kw:ward"
        assert projection[0].values["relations"] == Json(["has"])
        assert report["complete_cards"] == 1


def test_first_batch_pp_evolution_with_unproved_reminder_stays_partial(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    with case(
        tmp_path,
        monkeypatch,
        "{進化}{コスト1}：これは進化する。（合成の未証明注記）\n【合成甲】",
        first_batch=True,
    ) as (db, inputs, settings, _):
        with db.transaction():
            report = mechanics.apply(db, inputs, settings)
        coverage = db.rows("card_mechanic_coverage")[0].values
        assert coverage["complete_all"] is False
        assert coverage["complete_keyword_ids"] == Json([])
        assert coverage["partial_keyword_ids"] == Json(["kw:ward"])
        assert report["complete_cards"] == 0


_T10_QUOTED_FIELD = (
    "【スタック】（【スタック】を持つアミュレットは、下記の能力を持つ。"
    "「これはスタックカウンター1個を置いた状態で場に出る」"
    "「これが場を離れる際、代わりにこれのスタックカウンター1個を取る」"
    "「これのスタックカウンターが0個なら、これを墓場に置く」"
    "「{起動}これを{アクト}：自分の場の他の【スタック】を持つアミュレット1つを選ぶ。"
    "それにこれのスタックカウンターすべてを移す」）"
)


@pytest.mark.parametrize(
    "raw",
    [
        pytest.param("相手のフォロワー1体を選ぶ。それに2ダメージ。", id="follower"),
        pytest.param(
            "{ファンファーレ}相手のリーダー1人か相手のフォロワー1体を選ぶ。"
            "それに7ダメージ。",
            id="fanfare-choice",
        ),
        pytest.param(
            "{ファンファーレ}相手のフォロワー1体を選ぶ。それに4ダメージ。\n"
            "{起動}{コスト1}これを{アクト}：相手のフォロワー1体を選ぶ。"
            "それに4ダメージ。",
            id="fanfare-and-paid-activated",
        ),
        pytest.param(
            "{起動}{コスト0}これを{アクト}："
            "相手のリーダー1人か相手の場のフォロワー1体を選ぶ。それに10ダメージ。",
            id="field-choice-and-multidigit",
        ),
        pytest.param("相手のフォロワーすべてに4ダメージ。", id="opposing-all"),
        pytest.param("フォロワーすべてに5ダメージ。", id="both-all"),
        pytest.param(
            "{進化}{コスト2}：これは進化する。\n"
            "{ファンファーレ}相手の場のフォロワーすべてに2ダメージ。",
            id="evolution-and-field-all",
        ),
        pytest.param(
            "{ファンファーレ}自分の他のフォロワーすべてに2ダメージ。",
            id="own-other-all",
        ),
        pytest.param("{ファンファーレ}リーダーすべてに1ダメージ。", id="leaders"),
        pytest.param("{ラストワード}リーダーすべてに2ダメージ。", id="last-words"),
        pytest.param(
            "自分のエンドフェイズが来たとき、相手のリーダーすべてに4ダメージ。",
            id="end-phase-damage",
        ),
        pytest.param(
            "{ファンファーレ}自分のリーダーに1ダメージ。1枚引く。\n"
            "{起動}{コスト2}これを{アクト}墓場に置く：自分のリーダーに1ダメージ。1枚引く。",
            id="self-damage-draw-and-sacrifice",
        ),
        pytest.param(
            "相手のリーダー1人か相手の場のフォロワー1体を選ぶ。それに3ダメージ。1枚引く。",
            id="target-damage-draw",
        ),
        pytest.param("{ラストワード}1枚引く。", id="last-words-draw"),
        pytest.param(
            "【進化時】相手のフォロワー1体を選ぶ。それに2ダメージ。\n"
            "{ラストワード}1枚引く。",
            id="evolve-damage-and-last-words-draw",
        ),
        pytest.param(
            "{起動}これを{アクト}墓場に置く：1枚引く。\n"
            "{起動}{コスト2}これを{アクト}墓場に置く：2枚引く。",
            id="sacrifice-draw-with-and-without-pp",
        ),
        pytest.param("これが場を離れたとき、1枚引く。", id="leave-field-draw"),
        pytest.param(
            "{ファンファーレ}自分のリーダーは{体力}+5する。1枚引く。",
            id="recovery-and-draw",
        ),
        pytest.param("【進化時】自分のリーダーは{体力}+2する。", id="evolve-recovery"),
        pytest.param(
            "自分のエンドフェイズが来たとき、自分のリーダーは{体力}+4する。",
            id="end-phase-recovery",
        ),
    ],
)
def test_first_batch_fixed_effects_prove_absence_without_positive_labels(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, raw: str
) -> None:
    with case(tmp_path, monkeypatch, raw, first_batch=True) as (
        db,
        inputs,
        settings,
        _,
    ):
        support = db.rows("card_engine_support")
        with db.transaction():
            report = mechanics.apply(db, inputs, settings)
        assert db.rows("card_mechanic_coverage")[0].values["complete_all"] is True
        assert report["complete_cards"] == 1
        assert not db.rows("mechanic_projection")
        assert not db.rows("card_mechanic")
        assert db.rows("card_engine_support") == support


@pytest.mark.parametrize(
    "unproved",
    [
        pytest.param(
            "{ファンファーレ}【合成甲】を持つ相手のフォロワー1体を選ぶ。"
            "それに2ダメージ。",
            id="keyword-target",
        ),
        pytest.param(
            "{起動}{コスト1}手札の【合成甲】を持つフォロワー1枚を捨てる："
            "相手のフォロワー1体を選ぶ。それに2ダメージ。",
            id="keyword-cost",
        ),
        pytest.param(
            "{ファンファーレ}自分の場にフォロワーがいるなら、"
            "相手のフォロワー1体を選ぶ。それに2ダメージ。",
            id="condition",
        ),
        pytest.param(
            "相手のフォロワー1体を選ぶ。それに2ダメージ。1枚捨てる。",
            id="trailing-effect",
        ),
        pytest.param(
            "1枚引く。相手のフォロワー1体を選ぶ。それに2ダメージ。",
            id="leading-effect",
        ),
        pytest.param(
            "相手のフォロワー1体を選ぶ。それに2ダメージ。（未証明の注記）",
            id="reminder",
        ),
        pytest.param(
            "相手のフォロワー1体を選ぶ。それに２ダメージ。",
            id="fullwidth-damage",
        ),
        pytest.param(
            "相手のフォロワー1体を選ぶ。それにXダメージ。",
            id="variable-damage",
        ),
        pytest.param(
            "{起動}{コスト１}これを{アクト}："
            "相手のフォロワー1体を選ぶ。それに2ダメージ。",
            id="fullwidth-cost",
        ),
        pytest.param(
            "{起動}{コスト1}これを{アクト}手札1枚を捨てる："
            "相手のフォロワー1体を選ぶ。それに2ダメージ。",
            id="additional-cost",
        ),
        pytest.param(
            "{スタック}\n相手のフォロワー1体を選ぶ。それに2ダメージ。",
            id="unproved-ability-line",
        ),
        pytest.param(
            "これは「\n相手のフォロワー1体を選ぶ。それに2ダメージ。\n」を持つ。",
            id="quoted-ability",
        ),
        pytest.param(
            "【攻撃時】相手のフォロワーすべてに3ダメージ。",
            id="unproved-trigger",
        ),
        pytest.param(
            "相手のフォロワーすべてに2ダメージ。【覚醒】状態なら、代わりに3ダメージ。",
            id="conditional-replacement",
        ),
        pytest.param(
            "{ファンファーレ}【合成甲】を持つフォロワーすべてに2ダメージ。",
            id="keyword-all-target",
        ),
        pytest.param("リーダーすべてに２ダメージ。", id="all-fullwidth"),
        pytest.param("リーダーすべてにXダメージ。", id="all-variable"),
        pytest.param("相手のリーダーすべてに1ダメージ。仮。", id="all-extra-effect"),
        pytest.param("{スタック}\n相手のフォロワーすべてに1ダメージ。", id="all-stack"),
        pytest.param("{ラストワード}1枚引く。（未証明の注記）", id="draw-reminder"),
        pytest.param("【合成乙】を持つなら、1枚引く。", id="draw-keyword-condition"),
        pytest.param("相手は1枚引く。", id="draw-other-player"),
        pytest.param("X枚引く。", id="draw-variable"),
        pytest.param("１枚引く。", id="draw-fullwidth"),
        pytest.param(
            "1枚引く。自分の手札1枚を捨てる。", id="draw-discard-without-head"
        ),
        pytest.param(
            "{起動}手札の【合成乙】を持つカード1枚を捨てる：1枚引く。",
            id="draw-keyword-cost",
        ),
        pytest.param(
            "{起動}これを{アクト}墓場に置く：1枚引く。この能力は1ターンに1回使える。",
            id="activation-limit",
        ),
        pytest.param(
            "自分のリーダーは{体力}+2して、【合成乙】を持つ。",
            id="recovery-keyword-grant",
        ),
        pytest.param(
            "【合成乙】を持つなら、自分のリーダーは{体力}+2する。",
            id="recovery-condition",
        ),
        pytest.param("自分のリーダーは{体力}+Xする。", id="recovery-variable"),
        pytest.param("自分のリーダーは{体力}+２する。", id="recovery-fullwidth"),
        pytest.param("自分のリーダーは{体力}+2する。仮。", id="recovery-extra-effect"),
        pytest.param("これは「\n{ラストワード}1枚引く。\n」を持つ。", id="quoted-draw"),
        pytest.param(
            "これは「\n自分のリーダーは{体力}+2する。\n」を持つ。", id="quoted-recovery"
        ),
        pytest.param("{ラストワード}1枚引く。1枚引く。", id="draw-twice"),
        pytest.param(
            "相手のフォロワーすべてに1ダメージ。1枚引く。1枚引く。",
            id="damage-draw-twice",
        ),
        pytest.param(
            "自分のリーダーは{体力}+2する。自分のリーダーに1ダメージ。",
            id="recovery-then-damage",
        ),
        pytest.param("自分のフォロワーすべてに1ダメージ。", id="own-all-followers"),
        pytest.param(
            "相手のエンドフェイズが来たとき、1枚引く。", id="opponent-end-phase"
        ),
        pytest.param("{起動}これを{アクト}：1枚引く。", id="act-without-pp"),
        pytest.param(
            "{起動}{コスト1}墓場に置く：1枚引く。", id="graveyard-without-act"
        ),
        pytest.param(
            "{起動}{コスト１}これを{アクト}墓場に置く：1枚引く。",
            id="fullwidth-graveyard-cost",
        ),
    ],
)
def test_fixed_effects_cannot_close_unproved_content_with_a_positive_label(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, unproved: str
) -> None:
    with case(tmp_path, monkeypatch, "【合成甲】\n" + unproved, first_batch=True) as (
        db,
        inputs,
        settings,
        _,
    ):
        with db.transaction():
            report = mechanics.apply(db, inputs, settings)
        coverage = db.rows("card_mechanic_coverage")[0].values
        assert coverage["complete_all"] is False
        assert coverage["complete_keyword_ids"] == Json([])
        assert report["complete_cards"] == 0
        assert any(
            row.values["keyword_id"] == "kw:ward"
            and row.values["relations"] == Json(["has"])
            for row in db.rows("mechanic_projection")
        )


def test_fixed_damage_composes_with_evolution_and_self_declaration(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    with case(
        tmp_path,
        monkeypatch,
        "{進化}{コスト1}：これは進化する。\n【合成甲】\n"
        "{ファンファーレ}自分の他のフォロワーすべてに2ダメージ。",
        back="{ラストワード}1枚引く。",
        first_batch=True,
    ) as (db, inputs, settings, _):
        with db.transaction():
            mechanics.apply(db, inputs, settings)
        assert db.rows("card_mechanic_coverage")[0].values["complete_all"] is True
        projection = db.rows("mechanic_projection")
        assert len(projection) == 1
        assert projection[0].values["relations"] == Json(["has"])


@pytest.mark.parametrize(
    "raw",
    [
        (
            "{進化}{コスト0}：これは進化する。\n【突進】\n"
            "{ファンファーレ}これは{攻撃力}+Xする。"
            "Xは「これを含めず、このターン中に自分がプレイしたカードの枚数」である。"
        ),
        (
            "{クイック}\n"
            "相手のリーダー1人か相手のフォロワー1体を選ぶ。1枚引く。"
            "それに「自分の手札の枚数」と同じダメージ。"
        ),
        (
            "【突進】\n"
            "{ファンファーレ}他のフォロワー1体を選ぶ。"
            "これは{攻撃力}+Xする。Xは「選んだフォロワーの攻撃力」である。"
        ),
        (
            "【進化時】自分のリーダーに2ダメージ。"
            "相手プレイヤー1人は手札を公開する。"
            "自分はその中から1枚を選ぶ。相手はそれを捨てる。"
        ),
        _T10_QUOTED_FIELD,
    ],
    ids=[
        "bp01-004",
        "bp01-007",
        "bp01-095",
        "bp01-111-p23",
        "bp01-t10-four-stack-reminders",
    ],
)
def test_quoted_and_public_hand_fields_are_proved_only_as_exact_whole_fields(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, raw: str
) -> None:
    with case(tmp_path, monkeypatch, raw, first_batch=True) as (
        db,
        inputs,
        settings,
        _,
    ):
        with db.transaction():
            report = mechanics.apply(db, inputs, settings)
        assert report["complete_cards"] == 1
        assert db.rows("card_mechanic_coverage")[0].values["complete_all"] is True
        assert db.rows("mechanic_projection") == ()


@pytest.mark.parametrize(
    "raw",
    [
        (
            "{クイック}\n"
            "相手のリーダー1人か相手のフォロワー1体を選ぶ。1枚引く。"
            "それに「自分の手札の枚数」と同じダメージ。1枚引く。"
        ),
        (
            "【進化時】下記から1つチョイスする。"
            "【1】これは【疾走】を持つ。"
            "【2】相手のフォロワー1体を選ぶ。それに「これの攻撃力」と同じダメージ。"
        ),
        (
            "{クイック}\n"
            "相手のリーダー1人か相手のフォロワー1体を選ぶ。1枚引く。"
            "それに「自分の場の手札枚数」と同じダメージ。"
        ),
        (
            "【進化時】自分のリーダーに2ダメージ。"
            "相手プレイヤー1人は手札を公開する。"
            "自分はその中から1枚を選ぶ。相手はそれを捨てる。1枚引く。"
        ),
        _T10_QUOTED_FIELD.replace("スタックカウンター1個", "スタックカウンター2個", 1),
        _T10_QUOTED_FIELD.replace("代わりに", "", 1),
        _T10_QUOTED_FIELD.replace(
            "スタックカウンターが0個", "スタックカウンターが1個", 1
        ),
        _T10_QUOTED_FIELD.replace(
            "スタックカウンターすべて", "スタックカウンター1つ", 1
        ),
    ],
    ids=[
        "quoted-extra-effect",
        "self-conditional-grant",
        "quoted-formula-change",
        "hand-discard-extra-effect",
        "bp01-t10-entry-counter-changed",
        "bp01-t10-leave-replacement-changed",
        "bp01-t10-empty-counter-condition-changed",
        "bp01-t10-counter-transfer-changed",
    ],
)
def test_quoted_and_public_hand_near_misses_do_not_close_the_field(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, raw: str
) -> None:
    with case(tmp_path, monkeypatch, raw, first_batch=True) as (
        db,
        inputs,
        settings,
        _,
    ):
        with db.transaction():
            mechanics.apply(db, inputs, settings)
        coverage = db.rows("card_mechanic_coverage")
        assert not coverage or coverage[0].values["complete_all"] is False


@pytest.mark.parametrize(
    "raw",
    [
        pytest.param(
            "{ファンファーレ}自分の場にフォロワーがいるなら、"
            "相手のフォロワーすべてに2ダメージ。",
            id="damage-condition",
        ),
        pytest.param("{ラストワード}1枚引く。仮。", id="draw-extra-effect"),
        pytest.param("自分のリーダーは{体力}+2する。（仮）", id="recovery-reminder"),
    ],
)
def test_unproved_fixed_effect_without_labels_remains_unknown(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, raw: str
) -> None:
    with case(tmp_path, monkeypatch, raw, first_batch=True) as (
        db,
        inputs,
        settings,
        _,
    ):
        with db.transaction():
            mechanics.apply(db, inputs, settings)
        assert not db.rows("card_mechanic_coverage")
        assert not db.rows("mechanic_projection")


@pytest.mark.parametrize(
    "raw",
    [
        "自分の場に他のフォロワーが出たとき、これは{攻撃力}+1/{体力}+1する。",
        (
            "自分の場に{ロイヤル}フォロワーが出たとき、それは{攻撃力}+1/{体力}+1する。\n"
            "{ファンファーレ}自分の{ロイヤル}フォロワーすべては{攻撃力}+1/{体力}+1する。"
        ),
        "{ファンファーレ}自分の他のフォロワーすべては{攻撃力}+1/{体力}+1する。",
        (
            "{進化}{コスト2}：これは進化する。\n"
            "自分の場に他のフォロワーが出たとき、これは{攻撃力}+1する。"
        ),
        "自分の場に他のフォロワーが出たとき、これは{攻撃力}+1する。",
        (
            "{ファンファーレ}自分の他のフォロワー1体を選ぶ。それは{攻撃力}+1する。\n"
            "{ラストワード}自分のフォロワー1体を選ぶ。それは{攻撃力}+1する。"
        ),
        (
            "【進化時】相手のフォロワーすべてに2ダメージ。"
            "自分の他のフォロワーすべては{体力}+2する。"
        ),
        (
            "自分のエンドフェイズが来たとき、自分のフォロワーすべては{攻撃力}+1する。\n"
            "{ファンファーレ}自分のフォロワー1体を選ぶ。それは{攻撃力}+1する。"
        ),
        "自分のフォロワー1体を選ぶ。それは{攻撃力}+2する。",
        (
            "{ファンファーレ}自分の場の他の妖精・フォロワーすべてと、"
            "自分のEXエリアの妖精・フォロワーすべては{攻撃力}+1/{体力}+1する。"
        ),
        (
            "{進化}{コスト1}：これは進化する。\n"
            "{ファンファーレ}【覚醒】状態なら、これは{攻撃力}+3/{体力}+3する。"
        ),
        "{ファンファーレ}【真紅】状態なら、これは{攻撃力}+1/{体力}+1する。",
        (
            "{クイック}\n自分のフォロワー1体を選ぶ。"
            "それは{攻撃力}+1/{体力}+1する。1枚引く。"
        ),
        (
            "{クイック}\n自分のリーダーに2ダメージ。\n"
            "相手のリーダー1人か相手のフォロワー1体を選ぶ。それに3ダメージ。"
        ),
        "{クイック}\n相手のフォロワー1体を選ぶ。それに2ダメージ。",
        "{クイック}\n相手のフォロワーすべてに1ダメージ。",
        (
            "{ファンファーレ}自分の他のフォロワー1体を選ぶ。"
            "それは{攻撃力}+10/{体力}+0する。12枚引く。"
        ),
    ],
    ids=[f"stats-{index}" for index in range(17)],
)
def test_first_batch_stats_and_quick_prove_absence_without_labels(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, raw: str
) -> None:
    with case(tmp_path, monkeypatch, raw, first_batch=True) as (
        db,
        inputs,
        settings,
        _,
    ):
        with db.transaction():
            report = mechanics.apply(db, inputs, settings)
        assert db.rows("card_mechanic_coverage")[0].values["complete_all"] is True
        assert report["complete_cards"] == 1
        assert not db.rows("card_mechanic")
        assert not db.rows("mechanic_projection")


@pytest.mark.parametrize(
    "raw",
    [
        "{ファンファーレ}自分の【合成乙】を持つフォロワーすべては{攻撃力}+1する。",
        "自分の場に【合成乙】を持つフォロワーが出たとき、これは{攻撃力}+1する。",
        "{ファンファーレ}【合成乙】状態なら、これは{攻撃力}+1する。",
        "{ファンファーレ}【真紅】状態なら、これは{攻撃力}+1/{体力}+1して、【突進】を持つ。",
        "{ファンファーレ}【覚醒】状態なら、これは{攻撃力}+Xする。",
        "{ファンファーレ}【真紅】状態なら、これは{攻撃力}+１する。",
        "自分のフォロワー1体を選ぶ。それは{攻撃力}+Xする。",
        "自分のフォロワー1体を選ぶ。それは{攻撃力}+１する。",
        "自分のフォロワー1体を選ぶ。それは{攻撃力}-1する。",
        "自分のフォロワー1体を選ぶ。それは{攻撃力}+1する。（未証明）",
        "自分のフォロワー1体を選ぶ。それは{攻撃力}+1する。仮。",
        "自分のフォロワー1体を選ぶ。それは{攻撃力}+1する。1枚引く。1枚引く。",
        "1枚引く。自分のフォロワー1体を選ぶ。それは{攻撃力}+1する。",
        "{起動}{コスト1}これを{アクト}：自分のフォロワー1体を選ぶ。それは{攻撃力}+1する。",
        "【進化時】場の他のカード1枚を手札に戻す：これは{攻撃力}+1/{体力}+1する。仮。",
        "自分のフォロワーすべては{攻撃力}+1する。1枚引く。",
        "相手のフォロワーすべては{攻撃力}+1する。",
        "自分の場に他のフォロワーが出たとき、それは{攻撃力}+1する。",
        "自分の場に{ロイヤル}フォロワーが出たとき、これは{攻撃力}+1する。",
        "相手のエンドフェイズが来たとき、自分のフォロワーすべては{攻撃力}+1する。",
        "【進化時】相手のフォロワーすべてに2ダメージ。自分のリーダーは{体力}+2する。",
        "これは「\n自分のフォロワー1体を選ぶ。それは{攻撃力}+1する。\n」を持つ。",
        "これは「\n{クイック}\n」を持つ。",
        "{クイック}相手のフォロワーすべてに1ダメージ。",
        "{クイック}\n相手のフォロワーすべてに1ダメージ。仮。",
        "{クイック}\n自分のフォロワー1体を選ぶ。それは【合成乙】を持つ。仮。",
        (
            "{ファンファーレ}自分の場の他の妖精・フォロワーすべてと、\n"
            "自分のEXエリアの妖精・フォロワーすべては{攻撃力}+1/{体力}+1する。"
        ),
        "{ファンファーレ}自分の場の他の妖精・フォロワーすべては{攻撃力}+1/{体力}+1する。",
        (
            "【進化時】自分の他のフォロワーすべては{体力}+2する。"
            "相手のフォロワーすべてに2ダメージ。"
        ),
        (
            "【進化時】相手のフォロワーすべてに２ダメージ。"
            "自分の他のフォロワーすべては{体力}+2する。"
        ),
        "{クイック}{クイック}",
    ],
    ids=[f"unproved-stats-{index}" for index in range(31)],
)
@pytest.mark.parametrize("positive", [False, True])
def test_stats_and_quick_leave_unproved_fields_open(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, raw: str, positive: bool
) -> None:
    with case(
        tmp_path,
        monkeypatch,
        ("【合成甲】\n" if positive else "") + raw,
        first_batch=True,
    ) as (db, inputs, settings, _):
        with db.transaction():
            report = mechanics.apply(db, inputs, settings)
        assert report["complete_cards"] == 0
        coverage = db.rows("card_mechanic_coverage")
        if positive or coverage:
            assert coverage[0].values["complete_all"] is False
            assert coverage[0].values["complete_keyword_ids"] == Json([])
        else:
            assert not db.rows("mechanic_projection")


@pytest.mark.parametrize(
    "raw",
    [
        "【進化時】アクト状態の相手のフォロワー1体を選ぶ。それを破壊する。",
        "{クイック}\n自分のフォロワー1体を選ぶ。それを破壊する。2枚引く。",
        "場のフォロワーすべてを破壊する。",
        "{クイック}\n体力3以下の相手のフォロワー1体を選ぶ。それを消滅させる。",
        (
            "{クイック}\n自分の場のカード1枚を選ぶ。それを破壊する。"
            "自分のリーダーは{体力}+3する。1枚引く。"
        ),
        "【進化時】相手のEXエリアのカード1枚を選ぶ。それを消滅させる。",
        "{クイック}\n相手の場のカード1枚を選ぶ。それを破壊する。",
        "【攻撃時】1枚引く。自分の手札1枚を捨てる。",
        "{進化}{コスト2}：これは進化する。\n{ファンファーレ}2枚引く。自分の手札1枚を捨てる。",
        "{ファンファーレ}1枚引く。自分の手札1枚をデッキの下に置く。",
        "【進化時】自分のデッキから{進化}を持つフォロワー1枚を探し、手札に加える。",
        "{ファンファーレ}自分のデッキから{ロイヤル}フォロワー1枚を探し、手札に加える。",
        "{ファンファーレ}自分のデッキから忍者・カード1枚を探し、手札に加える。",
        "{進化}{コスト2}：これは進化する。\n{ファンファーレ}自分のデッキからスペル1枚を探し、手札に加える。",
        "【進化時】自分のデッキからアミュレット1枚を探し、手札に加える。",
        "体力10以下の相手のフォロワー1体を選ぶ。それを消滅させる。",
        "自分の場のカード1枚を選ぶ。それを破壊する。自分のリーダーは{体力}+0する。12枚引く。",
        "【攻撃時】12枚引く。自分の手札0枚を捨てる。",
        "{ファンファーレ}10枚引く。自分の手札2枚をデッキの下に置く。",
    ],
    ids=[f"movement-{index}" for index in range(19)],
)
@pytest.mark.parametrize("positive", [False, True])
def test_bounded_movement_proves_all_relations(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, raw: str, positive: bool
) -> None:
    with case(
        tmp_path,
        monkeypatch,
        ("【合成甲】\n" if positive else "") + raw,
        first_batch=True,
    ) as (db, inputs, settings, _):
        with db.transaction():
            report = mechanics.apply(db, inputs, settings)
        assert report["complete_cards"] == 1
        assert db.rows("card_mechanic_coverage")[0].values["complete_all"] is True
        projection = db.rows("mechanic_projection")
        if positive:
            assert len(projection) == 1
            assert projection[0].values["keyword_id"] == "kw:ward"
            assert projection[0].values["relations"] == Json(["has"])
        else:
            assert not projection
            assert not db.rows("card_mechanic")


@pytest.mark.parametrize(
    "raw",
    [
        "【進化時】スタンド状態の相手のフォロワー1体を選ぶ。それを破壊する。",
        "【進化時】【合成乙】を持つ相手のフォロワー1体を選ぶ。それを破壊する。",
        "相手のフォロワー1体を選ぶ。それを破壊する。【スペルチェイン_10】それのリーダーに3ダメージ。仮。",
        "{起動}これを{アクト}：相手の場のカード1枚を選ぶ。それを破壊する。",
        "体力３以下の相手のフォロワー1体を選ぶ。それを消滅させる。",
        "体力X以下の相手のフォロワー1体を選ぶ。それを消滅させる。",
        "体力3以下の相手のフォロワー1体まで選ぶ。それを消滅させる。",
        "体力3以下の相手のフォロワー1体を選ぶ。それを破壊する。",
        "場の【合成乙】を持つフォロワーすべてを破壊する。",
        "場のフォロワーすべてを破壊する。1枚引く。",
        "自分のフォロワー1体を選ぶ。それを破壊する。２枚引く。",
        "自分の場のカード1枚を選ぶ。それを破壊する。1枚引く。自分のリーダーは{体力}+3する。",
        "【進化時】相手のEXエリアの【合成乙】を持つカード1枚を選ぶ。それを消滅させる。",
        "相手の場のカード1枚を選ぶ。それを破壊する。（未証明）",
        "相手の場のカード1枚を選ぶ。それを破壊する。仮。",
        "これは「\n場のフォロワーすべてを破壊する。\n」を持つ。",
        "【攻撃時】1枚引く。自分の手札の【合成乙】を持つカード1枚を捨てる。",
        "【攻撃時】1枚引く。相手の手札1枚を捨てる。",
        "【攻撃時】1枚引く。自分の手札１枚を捨てる。",
        "【攻撃時】1枚引く。自分の手札X枚を捨てる。",
        "【攻撃時】1枚引く。自分の手札1枚を捨てる。この能力は1ターンに1回使える。",
        "【攻撃時】自分の手札1枚を捨てる：1枚引く。",
        "{ファンファーレ}1枚引く。自分の手札1枚をデッキの上に置く。",
        "{ファンファーレ}1枚引く。自分の手札1枚をデッキの下に置いてよい。",
        "{ファンファーレ}自分の手札1枚をデッキの下に置く。1枚引く。",
        "【進化時】自分のデッキから【合成乙】を持つフォロワー1枚を探し、手札に加える。",
        "【進化時】相手のデッキからアミュレット1枚を探し、手札に加える。",
        "【進化時】自分のデッキからアミュレット2枚を探し、手札に加える。",
        "【進化時】自分のデッキからアミュレット1枚を探し、場に出す。",
        "{ファンファーレ}【覚醒】状態なら、自分のデッキからスペル1枚を探し、手札に加える。",
        "{ファンファーレ}自分のデッキからスペル1枚を探し、手札に加える。仮。",
        "{ファンファーレ}自分のデッキからスペル1枚を探し、手札に加える。（未証明）",
        "【進化時】自分のデッキから{進化}を持つフォロワー１枚を探し、手札に加える。",
        "{ファンファーレ}自分のデッキからスペル1枚を探し、\n手札に加える。",
        "【進化時】場のフォロワーすべてを破壊する。",
        "{ファンファーレ}相手の場のカード1枚を選ぶ。それを破壊する。",
        "【進化時】自分のフォロワー1体を選ぶ。それを破壊する。1枚引く。",
        "【進化時】1枚引く。自分の手札1枚を捨てる。",
        "【攻撃時】1枚引く。自分の手札1枚をデッキの下に置く。",
        "【進化時】自分のデッキから忍者・カード1枚を探し、手札に加える。",
        "{ファンファーレ}自分のデッキからアミュレット1枚を探し、手札に加える。",
        "{ファンファーレ}自分のデッキから{進化}を持つフォロワー1枚を探し、手札に加える。",
    ],
    ids=[f"unproved-movement-{index}" for index in range(42)],
)
@pytest.mark.parametrize("positive", [False, True])
def test_bounded_movement_leaves_unproved_fields_open(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, raw: str, positive: bool
) -> None:
    with case(
        tmp_path,
        monkeypatch,
        ("【合成甲】\n" if positive else "") + raw,
        first_batch=True,
    ) as (db, inputs, settings, _):
        with db.transaction():
            report = mechanics.apply(db, inputs, settings)
        assert report["complete_cards"] == 0
        coverage = db.rows("card_mechanic_coverage")
        if positive or coverage:
            assert coverage[0].values["complete_all"] is False
            assert coverage[0].values["complete_keyword_ids"] == Json([])
        else:
            assert not db.rows("mechanic_projection")


@pytest.mark.parametrize(
    "raw",
    [
        "これが場から手札に戻ったとき、相手のフォロワー1体を選ぶ。それに1ダメージ。\n{ファンファーレ}相手のフォロワー1体を選ぶ。それに1ダメージ。",
        "{進化}{コスト1}：これは進化する。\n自分の場に他のフォロワーが出たとき、相手のフォロワー1体を選ぶ。それに1ダメージ。",
        "自分の場に他のフォロワーが出たとき、相手のフォロワー2体まで選ぶ。それに1ダメージ。",
        "自分の妖精・フォロワーが攻撃するとき、相手のリーダー1人か相手のフォロワー1体を選ぶ。それに1ダメージ。",
        "相手のフォロワーが破壊されたとき、それのリーダーに1ダメージ。自分のリーダーは{体力}+1する。",
        "{起動}これを{アクト}墓場に置く：自分の墓場のコスト3以上のフォロワー1枚を選ぶ。それを手札に加える。",
        "{ファンファーレ}自分の墓場のフォロワー2枚まで選ぶ。それを手札に加える。",
        "自分の墓場のコスト5以下のアミュレット3枚まで選ぶ。それを場に出す。",
        "{ファンファーレ}自分の墓場のコスト5以下のアミュレット1枚を選ぶ。それを場に出す。",
        "{ファンファーレ}自分のデッキの上2枚を墓場に置く。",
        "{進化}{コスト2}：これは進化する。\n{ファンファーレ}相手のフォロワー1体を選ぶ。それをアクトする。",
        "{ファンファーレ}相手のフォロワー1体を選ぶ。それをアクトする。\n{起動}これを{アクト}：相手のフォロワー1体を選ぶ。それをアクトする。",
        "これはアクト状態で場に出る。\n{起動}これを{アクト}墓場に置く：相手のフォロワー1体を選ぶ。それを破壊する。",
        "自分のエンドフェイズが来たとき、これは消滅する。",
        "【進化時】フォロワー1体を墓場に置く：相手のフォロワー1体を選ぶ。それを破壊する。",
        "相手のフォロワーが破壊されたとき、それのリーダーに10ダメージ。自分のリーダーは{体力}+0する。",
        "{起動}これを{アクト}墓場に置く：自分の墓場のコスト10以上のフォロワー1枚を選ぶ。それを手札に加える。",
        "{ファンファーレ}自分のデッキの上12枚を墓場に置く。",
    ],
    ids=[
        f"bounded_trigger_grave_act_proves_all_relations-{index}" for index in range(18)
    ],
)
@pytest.mark.parametrize("positive", [False, True])
def test_bounded_trigger_grave_act_proves_all_relations(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, raw: str, positive: bool
) -> None:
    with case(
        tmp_path,
        monkeypatch,
        ("【合成甲】\n" if positive else "") + raw,
        first_batch=True,
    ) as (db, inputs, settings, _):
        with db.transaction():
            report = mechanics.apply(db, inputs, settings)
        assert report["complete_cards"] == 1
        assert db.rows("card_mechanic_coverage")[0].values["complete_all"] is True
        projection = db.rows("mechanic_projection")
        if positive:
            assert len(projection) == 1
            assert projection[0].values["keyword_id"] == "kw:ward"
            assert projection[0].values["relations"] == Json(["has"])
        else:
            assert not projection
            assert not db.rows("card_mechanic")


@pytest.mark.parametrize(
    "raw",
    [
        "これが墓場から手札に戻ったとき、相手のフォロワー1体を選ぶ。それに1ダメージ。",
        "自分の場に【合成乙】を持つフォロワーが出たとき、相手のフォロワー1体を選ぶ。それに1ダメージ。",
        "自分の場に他のフォロワーが出たとき、相手のフォロワー3体まで選ぶ。それに1ダメージ。",
        "自分の場に他のフォロワーが出たとき、相手のフォロワー2体を選ぶ。それに1ダメージ。",
        "自分の【合成乙】を持つフォロワーが攻撃するとき、相手のリーダー1人か相手のフォロワー1体を選ぶ。それに1ダメージ。",
        "自分の妖精・フォロワーが攻撃したとき、相手のリーダー1人か相手のフォロワー1体を選ぶ。それに1ダメージ。",
        "相手のフォロワーが破壊されたとき、相手のリーダーに1ダメージ。自分のリーダーは{体力}+1する。",
        "相手のフォロワーが破壊されたとき、それのリーダーに１ダメージ。自分のリーダーは{体力}+1する。",
        "相手のフォロワーが破壊されたとき、それのリーダーに1ダメージ。自分のリーダーは{体力}+Xする。",
        "{起動}これを{アクト}：自分の墓場のコスト3以上のフォロワー1枚を選ぶ。それを手札に加える。",
        "{起動}これを{アクト}墓場に置く：自分の墓場の元のコスト3以上のフォロワー1枚を選ぶ。それを手札に加える。",
        "{起動}これを{アクト}墓場に置く：自分の墓場のコスト3以下のフォロワー1枚を選ぶ。それを手札に加える。",
        "{起動}これを{アクト}墓場に置く：相手の墓場のコスト3以上のフォロワー1枚を選ぶ。それを手札に加える。",
        "{起動}これを{アクト}墓場に置く：自分の墓場のコスト3以上の【合成乙】を持つフォロワー1枚を選ぶ。それを手札に加える。",
        "{起動}これを{アクト}墓場に置く：自分の墓場のコスト3以上のフォロワー1枚を選ぶ。それを場に出す。",
        "{起動}{コスト1}これを{アクト}墓場に置く：自分の墓場のコスト3以上のフォロワー1枚を選ぶ。それを手札に加える。",
        "{ファンファーレ}自分の墓場のフォロワー3枚まで選ぶ。それを手札に加える。",
        "【進化時】自分の墓場のフォロワー2枚まで選ぶ。それを手札に加える。",
        "自分の墓場のコスト5以下のフォロワー3枚まで選ぶ。それを場に出す。",
        "自分の墓場のコスト5以下のアミュレット1枚を選ぶ。それを場に出す。",
        "自分の墓場のコスト5以下のアミュレット3枚まで選ぶ。それを手札に加える。",
        "自分の墓場のコスト５以下のアミュレット3枚まで選ぶ。それを場に出す。",
        "{ファンファーレ}自分の墓場のコスト5以下のアミュレット2枚まで選ぶ。それを場に出す。",
        "{ファンファーレ}相手のデッキの上2枚を墓場に置く。",
        "{ファンファーレ}自分のデッキの上X枚を墓場に置く。",
        "{ファンファーレ}自分のデッキの上2枚をEXエリアに置く。",
        "{ラストワード}自分のデッキの上2枚を墓場に置く。",
        "【進化時】相手のフォロワー1体を選ぶ。それをアクトする。",
        "{起動}{コスト1}これを{アクト}：相手のフォロワー1体を選ぶ。それをアクトする。仮。",
        "これはスタンド状態で場に出る。",
        "{起動}これを{アクト}：相手のフォロワー1体を選ぶ。それを破壊する。仮。",
        "{起動}これを{アクト}墓場に置く：相手のフォロワー1体を選ぶ。それを消滅させる。",
        "相手のエンドフェイズが来たとき、これは消滅する。",
        "【進化時】【合成乙】を持つフォロワー1体を墓場に置く：相手のフォロワー1体を選ぶ。それを破壊する。",
        "{ファンファーレ}フォロワー1体を墓場に置く：相手のフォロワー1体を選ぶ。それを破壊する。",
        "【進化時】フォロワー1体を破壊する：相手のフォロワー1体を選ぶ。それを破壊する。",
        "自分の場に他のフォロワーが出たとき、相手のフォロワー2体まで選ぶ。それに1ダメージ。仮。",
        "自分の場に他のフォロワーが出たとき、相手のフォロワー2体まで選ぶ。それに1ダメージ。（未証明）",
        "自分の場に他のフォロワーが出たとき、相手のフォロワー2体まで選ぶ。それに1ダメージ。この能力は1ターンに1回使える。",
        "自分の場に他のフォロワーが出たとき、相手のフォロワー2体まで選ぶ。\nそれに1ダメージ。",
        "これは「\n自分の場に他のフォロワーが出たとき、相手のフォロワー2体まで選ぶ。それに1ダメージ。\n」を持つ。",
        "{起動}これを{アクト}墓場に置く：自分の墓場のコスト3以上のフォロワー1枚を選ぶ。それを手札に加える。仮。",
        "{起動}これを{アクト}墓場に置く：自分の墓場のコスト3以上のフォロワー1枚を選ぶ。それを手札に加える。（未証明）",
        "{起動}これを{アクト}墓場に置く：自分の墓場のコスト3以上のフォロワー1枚を選ぶ。それを手札に加える。この能力は1ターンに1回使える。",
        "{起動}これを{アクト}墓場に置く：自分の墓場のコスト3以上のフォロワー1枚を選ぶ。\nそれを手札に加える。",
        "これは「\n{起動}これを{アクト}墓場に置く：自分の墓場のコスト3以上のフォロワー1枚を選ぶ。それを手札に加える。\n」を持つ。",
        "{起動}これを{アクト}：相手のフォロワー1体を選ぶ。それをアクトする。仮。",
        "{起動}これを{アクト}：相手のフォロワー1体を選ぶ。それをアクトする。（未証明）",
        "{起動}これを{アクト}：相手のフォロワー1体を選ぶ。それをアクトする。この能力は1ターンに1回使える。",
        "{起動}これを{アクト}：相手のフォロワー1体を選ぶ。\nそれをアクトする。",
        "これは「\n{起動}これを{アクト}：相手のフォロワー1体を選ぶ。それをアクトする。\n」を持つ。",
    ],
    ids=[
        f"bounded_trigger_grave_act_leaves_unproved_fields_open-{index}"
        for index in range(51)
    ],
)
@pytest.mark.parametrize("positive", [False, True])
def test_bounded_trigger_grave_act_leaves_unproved_fields_open(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, raw: str, positive: bool
) -> None:
    with case(
        tmp_path,
        monkeypatch,
        ("【合成甲】\n" if positive else "") + raw,
        first_batch=True,
    ) as (db, inputs, settings, _):
        with db.transaction():
            report = mechanics.apply(db, inputs, settings)
        assert report["complete_cards"] == 0
        coverage = db.rows("card_mechanic_coverage")
        if positive or coverage:
            assert coverage[0].values["complete_all"] is False
            assert coverage[0].values["complete_keyword_ids"] == Json([])
        else:
            assert not db.rows("mechanic_projection")


_BOUNDED_LINES = (
    "{ファンファーレ}自分の他のフォロワー1体を選ぶ。それは【合成乙】を持つ。\n"
    "【進化時】【覚醒】状態なら、1枚引く。\n"
    "これが受けるダメージを-1する。"
)


def test_conditional_state_and_bounded_grant_prove_exact_projection(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    with case(
        tmp_path, monkeypatch, "【合成甲】\n" + _BOUNDED_LINES, first_batch=True
    ) as (db, inputs, settings, _):
        with db.transaction():
            report = mechanics.apply(db, inputs, settings)
        assert report["complete_cards"] == 1
        assert db.rows("card_mechanic_coverage")[0].values["complete_all"] is True
        assert {
            r.values["keyword_id"]: r.values["relations"]
            for r in db.rows("mechanic_projection")
        } == {"kw:ward": Json(["has"]), "kw:storm": Json(["grants"])}


def test_proved_labels_do_not_close_unknown_conditional_grant(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    raw = (
        "【合成甲】\n"
        + _BOUNDED_LINES
        + "\n{ファンファーレ}【真紅】状態なら、これは【合成乙】を持つ。"
    )
    with case(tmp_path, monkeypatch, raw, first_batch=True) as (
        db,
        inputs,
        settings,
        _,
    ):
        with db.transaction():
            report = mechanics.apply(db, inputs, settings)
        assert report["complete_cards"] == 0
        coverage = db.rows("card_mechanic_coverage")[0].values
        assert coverage["complete_all"] is False
        assert coverage["complete_keyword_ids"] == Json([])
        assert {
            r.values["keyword_id"]: r.values["relations"]
            for r in db.rows("mechanic_projection")
        } == {"kw:ward": Json(["has"]), "kw:storm": Json(["grants"])}


_LOOKCOST_LINES = (
    "自分のデッキの上4枚を見る。その中から、スペル1枚を公開して手札に加えてよい。"
    "スペル1枚を墓場に置いてよい。残りを好きな順にデッキの下に置く。\n"
    "【進化時】場の他のカード1枚を手札に戻す：これは{攻撃力}+1/{体力}+1する。\n"
    "相手のフォロワー1体を選ぶ。それを破壊する。【スペルチェイン_10】それのリーダーに3ダメージ。\n"
    "{起動}これを{アクト}：相手のフォロワー1体を選ぶ。それを破壊する。\n"
    "{起動}EXエリアの妖精・フォロワー1枚を消滅："
    "自分のリーダーは{体力}+3する。1枚引く。この能力は1ターンに1回使える。\n"
    "これをプレイする際、【スペルチェイン_5】コストを-3する。"
    "【SC_10】代わりに-6する。【SC_15】代わりに-9する。"
)


def test_deck_look_cost_and_adjustment_complete_without_new_positive_labels(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    with case(
        tmp_path, monkeypatch, "【合成甲】\n" + _LOOKCOST_LINES, first_batch=True
    ) as (db, inputs, settings, _):
        with db.transaction():
            report = mechanics.apply(db, inputs, settings)
        assert report["complete_cards"] == 1
        assert db.rows("card_mechanic_coverage")[0].values["complete_all"] is True
        assert {
            r.values["keyword_id"]: r.values["relations"]
            for r in db.rows("mechanic_projection")
        } == {"kw:ward": Json(["has"])}


def test_deck_look_keyword_filter_keeps_otherwise_proved_card_open(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    raw = "【合成甲】\n" + _LOOKCOST_LINES.replace(
        "スペル1枚を公開", "【合成甲】を持つフォロワー1枚を公開"
    )
    with case(tmp_path, monkeypatch, raw, first_batch=True) as (
        db,
        inputs,
        settings,
        _,
    ):
        with db.transaction():
            report = mechanics.apply(db, inputs, settings)
        assert report["complete_cards"] == 0
        coverage = db.rows("card_mechanic_coverage")[0].values
        assert coverage["complete_all"] is False
        assert coverage["complete_keyword_ids"] == Json([])
        assert {
            r.values["keyword_id"]: r.values["relations"]
            for r in db.rows("mechanic_projection")
        } == {"kw:ward": Json(["has"])}


_REMINDER_STATE_ZONE_LINES = (
    "自分の場のカード1枚を選ぶ。それを手札に戻す。1枚引く。（手札に戻ったトークンはゲームから取り除く）\n"
    "{進化}{コスト1}：これは進化する。\n"
    "【合成甲】\n"
    "{ファンファーレ}場の他のカード1枚を手札に戻す：これは{攻撃力}+1/{体力}+1する。（手札に戻ったトークンはゲームから取り除く）\n"
    "{クイック}\n"
    "相手の場のフォロワー1体を選ぶ。それに2ダメージ。【覚醒】状態なら、代わりに4ダメージ。(自分のPP最大値が7以上なら、【覚醒】状態になる)\n"
    "相手のフォロワー1体を選ぶ。それのプレイヤーはそれをデッキの上か下に置く。【コンボ_5】それをデッキに置く代わりに、相手のフォロワーすべてをそれのプレイヤーはデッキの上か下に置く。（1体ずつ上か下か決める）（デッキに置いたトークンはゲームから取り除く）\n"
    "{ファンファーレ}相手のフォロワー1体を選ぶ。それをEXエリアに置く。（EXエリアに置いたエボルヴフォロワーはゲームから取り除く）\n"
    "これがいる限り、自分のフォロワーは攻撃力ではなく体力と同じダメージを与える。\n"
    "これが場にいる限り、自分がゴーレム・フォロワーをプレイする際、コストを-1する。\n"
    "自分の場にゴーレム・フォロワーが出たとき、相手のリーダー1人か相手の場のフォロワー1体を選ぶ。それに3ダメージ。\n"
    "【攻撃時】相手のフォロワーすべてに3ダメージ。自分の墓場に『ツインプリズナー・フラム』と『ツインプリズナー・グラス』があるなら、代わりに7ダメージ。\n"
    "自分のエンドフェイズが来たとき、これは{体力}+2する。\n"
    "相手のフォロワー1体を選ぶ。このターン、それはダメージを与えない。\n"
    "各プレイヤーは、手札すべてをデッキに戻し、シャッフルし、戻した枚数と同じだけ引く。自分は、さらに1枚引く。\n"
    "これをプレイする際、場のスタンド状態の{ナイトメア}カード4枚を墓場に置く、または、EXエリアの{ナイトメア}カード4枚を消滅：コストを-9する。\n"
    "これをプレイする際、墓場のスペル10枚を消滅：コストを7にする。\n"
    "このターンのあと、自分の追加ターンを行う。\n"
    "{ファンファーレ}自分のデッキの上1枚をEXエリアに置く。それをプレイする際、コストを-2する。\n"
    "自分のデッキの上1枚をEXエリアに置く。これをEXエリアが上限になるまでくり返す。\n"
    "次に自分のエンドフェイズが来たとき、自分のEXエリアのカードすべてを消滅させる。"
)


def test_reminder_state_and_zone_replacements_keep_only_self_labels(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    with case(tmp_path, monkeypatch, _REMINDER_STATE_ZONE_LINES, first_batch=True) as (
        db,
        inputs,
        settings,
        _,
    ):
        with db.transaction():
            report = mechanics.apply(db, inputs, settings)
        assert report["complete_cards"] == 1
        assert db.rows("card_mechanic_coverage")[0].values["complete_all"] is True
        assert {
            r.values["keyword_id"]: r.values["relations"]
            for r in db.rows("mechanic_projection")
        } == {"kw:ward": Json(["has"])}


def test_unproved_reminder_keeps_otherwise_proved_field_open(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    raw = _REMINDER_STATE_ZONE_LINES.replace(
        "（手札に戻ったトークンはゲームから取り除く）",
        "（手札に戻ったトークンはゲームから取り除く。それは【合成乙】を持つ）",
    )
    with case(tmp_path, monkeypatch, raw, first_batch=True) as (
        db,
        inputs,
        settings,
        _,
    ):
        with db.transaction():
            report = mechanics.apply(db, inputs, settings)
        assert report["complete_cards"] == 0
        coverage = db.rows("card_mechanic_coverage")[0].values
        assert coverage["complete_all"] is False
        assert coverage["complete_keyword_ids"] == Json([])
        assert {
            r.values["keyword_id"]: r.values["relations"]
            for r in db.rows("mechanic_projection")
        } == {"kw:ward": Json(["has"])}


def test_named_tokens_in_ex_do_not_supply_parent_keyword_labels(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    raw = (
        "{ファンファーレ}『冥焔の右腕・ミミ』1枚か『冥焔の左腕・ココ』1枚をEXエリアに置く。"
        "【ネクロチャージ_10】代わりに1枚ずつ置く。"
    )
    with case(tmp_path, monkeypatch, raw, first_batch=True) as (
        db,
        inputs,
        settings,
        _,
    ):
        with db.transaction():
            report = mechanics.apply(db, inputs, settings)
        assert report["complete_cards"] == 1
        assert db.rows("card_mechanic_coverage")[0].values["complete_all"] is True
        assert db.rows("mechanic_projection") == ()


def test_named_token_field_with_unproved_remainder_stays_partial(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    raw = (
        "【合成甲】\n"
        "【進化時】『乙姫お守り隊』3枚を、場が上限になるまで出す。"
        "残りを【合成乙】を持つ状態でEXエリアに置く。"
    )
    with case(tmp_path, monkeypatch, raw, first_batch=True) as (
        db,
        inputs,
        settings,
        _,
    ):
        with db.transaction():
            report = mechanics.apply(db, inputs, settings)
        assert report["complete_cards"] == 0
        assert db.rows("card_mechanic_coverage")[0].values["complete_all"] is False
        assert {
            row.values["keyword_id"]: row.values["relations"]
            for row in db.rows("mechanic_projection")
        } == {"kw:ward": Json(["has"])}


def test_stack_declaration_and_spell_counter_do_not_add_tracked_keywords(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    raw = (
        "【スタック】\n"
        "自分がスペルをプレイしたとき、これにスペルカウンター1つを置く。\n"
        "{起動}これのスペルカウンター3つを取る：相手のフォロワー1体を選ぶ。それに5ダメージ。"
    )
    with case(tmp_path, monkeypatch, raw, first_batch=True) as (
        db,
        inputs,
        settings,
        _,
    ):
        with db.transaction():
            report = mechanics.apply(db, inputs, settings)
        assert report["complete_cards"] == 1
        assert db.rows("card_mechanic_coverage")[0].values["complete_all"] is True
        assert db.rows("mechanic_projection") == ()


def test_stack_quoted_reminder_is_not_proved_by_its_declaration(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    raw = "【合成甲】\n【スタック】（【スタック】を持つアミュレットは、下記の能力を持つ。「これは【合成乙】を持つ」）"
    with case(tmp_path, monkeypatch, raw, first_batch=True) as (
        db,
        inputs,
        settings,
        _,
    ):
        with db.transaction():
            report = mechanics.apply(db, inputs, settings)
        assert report["complete_cards"] == 0
        assert db.rows("card_mechanic_coverage")[0].values["complete_all"] is False
        assert {
            row.values["keyword_id"]: row.values["relations"]
            for row in db.rows("mechanic_projection")
        } == {"kw:ward": Json(["has"])}


def test_first_batch_missing_current_back_prevents_complete(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    with case(
        tmp_path,
        monkeypatch,
        "{進化}{コスト2}：これは進化する。\n【合成甲】\n"
        "相手のフォロワーすべてに2ダメージ。",
        back="【合成乙】",
        first_batch=True,
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


def test_other_card_grant_merges_with_self_declaration_and_completes(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    raw = (
        "{進化}{コスト2}：これは進化する。\n"
        "【合成甲】\n{ファンファーレ}自分の他のフォロワー1体を選ぶ。それは【合成甲】を持つ。"
    )
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
        assert coverage["complete_all"] is True
        assert coverage["partial_keyword_ids"] == Json([])
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
    with case(
        tmp_path,
        monkeypatch,
        "【合成甲】\n相手のフォロワー1体を選ぶ。それに2ダメージ。",
        first_batch=True,
    ) as (
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
    with case(tmp_path, monkeypatch, "{ラストワード}1枚引く。", first_batch=True) as (
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


@pytest.mark.parametrize(
    "raw",
    [
        "これを自分の手札から捨てたとき、7枚引く。",
        "{起動}{コスト7}これを{アクト}：相手のフォロワー1体を選ぶ。それをアクトする。",
        "{起動}これを{アクト}：フォロワー1体を選ぶ。それを破壊する。それのプレイヤーは『ドラゴン』5体を出す。",
        "{ラストワード}『大地の魔片』5つを出す。",
        "{ファンファーレ}下記から1つチョイスする。【1】【土の秘術】これは{攻撃力}+3/{体力}+4する。【2】『大地の魔片』5つを出す。",
        "{ファンファーレ}『フェアリー』5体を出す。『フェアリー』7枚をEXエリアに置く。",
        "『ゴースト』5体を出す。",
        "相手のフォロワー1体を選ぶ。それに7ダメージ。『ナイト』5枚をEXエリアに置く。",
        "【進化時】『フェアリーウィスプ』5枚をEXエリアに置く。",
        "『フェアリー』5枚をEXエリアに置く。",
        "{ファンファーレ}『乙姫お守り隊』5体を出す。",
        "『防御型ゴーレム』5体を出す。\n【土の秘術】自分のゴーレム・フォロワーすべては{攻撃力}+3/{体力}+4する。",
        "{ファンファーレ}【土の秘術】『防御型ゴーレム』5枚をEXエリアに置く。",
        "{ファンファーレ}『攻撃型ゴーレム』5体を出す。",
        "{ファンファーレ}【コンボ_7】相手のフォロワー1体を選ぶ。それを破壊する。相手の場に『フェアリー』5体を出す。",
        "『ヴァイキング』5体と『スティールナイト』7体と『ナイト』9体を出す。",
    ],
    ids=[
        "discard-draw",
        "costed-act",
        "destroy-and-dragon",
        "earth-fragment",
        "earth-choice",
        "fairy-field-ex",
        "ghost",
        "damage-and-knight",
        "wisp-ex",
        "fairy-ex",
        "escort",
        "earth-stats",
        "earth-ex",
        "attack-golem",
        "combo-opponent",
        "three-tokens",
    ],
)
@pytest.mark.parametrize("remainder", [False, True], ids=["whole", "unknown-tail"])
def test_section_card_bodies_require_every_clause(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, raw: str, remainder: bool
) -> None:
    with case(
        tmp_path,
        monkeypatch,
        raw + ("未知の効果。" if remainder else ""),
        first_batch=True,
        sections=("7枚引く。",),
    ) as (db, inputs, settings, _):
        with db.transaction():
            report = mechanics.apply(db, inputs, settings)
        assert report["complete_cards"] == (0 if remainder else 1)
        assert db.rows("mechanic_projection") == ()


_ROSE_FIELD = (
    "{ファンファーレ}自分のEXエリアの妖精・フォロワーを好きな枚数選ぶ。"
    "それは『薔薇の一撃』に変身する。\n"
    "{起動}これを{アクト}：自分のPPを「自分のEXエリアの『薔薇の一撃』の枚数」"
    "と同じだけ回復する。"
)


@pytest.mark.parametrize("suffix", ["", "\n未知の能力。", "それは【合成乙】を持つ。"])
def test_named_ex_count_quote_proves_only_the_entire_bounded_field(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, suffix: str
) -> None:
    with case(
        tmp_path,
        monkeypatch,
        _ROSE_FIELD + suffix,
        first_batch=True,
        sections=("7枚引く。",),
    ) as (db, inputs, settings, _):
        with db.transaction():
            report = mechanics.apply(db, inputs, settings)
        assert report["complete_cards"] == (0 if suffix else 1)
        assert db.rows("mechanic_projection") == ()


@pytest.mark.parametrize(
    "section",
    [
        "7枚引く。",
        "『フェアリー』{エルフ}妖精・フォロワー{コスト7}{攻撃力}8/{体力}9",
        "『攻撃型ゴーレム』{ウィッチ}ゴーレム・フォロワー{コスト7}{攻撃力}8/{体力}9【突進】",
        "『乙姫お守り隊』{ロイヤル}兵士・フォロワー{コスト7}{攻撃力}8/{体力}9【守護】",
        "『ゴースト』{ナイトメア}死者・フォロワー{コスト7}{攻撃力}8/{体力}9【疾走】自分のエンドフェイズが来たとき、これは消滅する。",
        "『大地の魔片』{ウィッチ}土の印・アミュレット{コスト7}【スタック】",
        "『薔薇の一撃』{エルフ}植物族・スペル{コスト7}相手のリーダー1人か相手のフォロワー1体を選ぶ。それに7ダメージ。8枚引く。",
        "『ヴァイキング』{ロイヤル}盗賊・フォロワー{コスト7}{攻撃力}8/{体力}9【疾走】\n『スティールナイト』{ロイヤル}兵士・フォロワー{コスト7}{攻撃力}8/{体力}9\n『ナイト』{ロイヤル}兵士・フォロワー{コスト7}{攻撃力}8/{体力}9",
    ],
    ids=["draw", "fairy", "rush", "ward", "ghost", "stack", "spell", "multi-token"],
)
@pytest.mark.parametrize("extra", ["", "【合成乙】", "\nそれは【合成乙】を持つ。"])
def test_section_proof_preserves_token_ownership_and_rejects_unknown_tails(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, section: str, extra: str
) -> None:
    with case(
        tmp_path,
        monkeypatch,
        "【合成甲】",
        first_batch=True,
        sections=(section + extra,),
    ) as (db, inputs, settings, _):
        with db.transaction():
            report = mechanics.apply(db, inputs, settings)
        assert report["complete_cards"] == (0 if extra else 1)
        assert {
            r.values["keyword_id"]: r.values["relations"]
            for r in db.rows("mechanic_projection")
        } == {"kw:ward": Json(["has"])}


@pytest.mark.parametrize("mutation", ["remove", "add", "gap", "reorder"])
def test_section_proof_requires_the_whole_ordered_frozen_list(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, mutation: str
) -> None:
    with case(
        tmp_path,
        monkeypatch,
        "【合成甲】",
        first_batch=True,
        sections=("7枚引く。", "8枚引く。"),
    ) as (db, inputs, settings, _):
        with db.transaction():
            rows = db.select(
                "face_text_section",
                db.columns("face_text_section"),
                where={"revision_id": "link-revision"},
            )
            key: dict[str, Value] = {"revision_id": "link-revision", "ordinal": 1}
            if mutation == "remove":
                db.delete("face_text_section", key)
            elif mutation == "add":
                db.insert("face_text_section", dict(rows[1].values) | {"ordinal": 2})
            elif mutation == "gap":
                db.update("face_text_section", key, {"ordinal": 2})
            else:
                db.update(
                    "face_text_section",
                    key,
                    {"text_unit_id": rows[0].values["text_unit_id"]},
                )
                db.update(
                    "face_text_section",
                    {"revision_id": "link-revision", "ordinal": 0},
                    {"text_unit_id": rows[1].values["text_unit_id"]},
                )
            report = mechanics.apply(db, inputs, settings)
        assert report["complete_cards"] == 0
        assert db.rows("card_mechanic_coverage")[0].values["complete_all"] is False


@pytest.mark.parametrize("back", [None, "未知の効果。"])
def test_known_sections_never_close_an_unproved_main_or_other_face(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, back: str | None
) -> None:
    with case(
        tmp_path,
        monkeypatch,
        "未知の効果。" if back is None else "【合成甲】",
        back=back,
        first_batch=True,
        sections=("7枚引く。",),
    ) as (db, inputs, settings, _):
        with db.transaction():
            report = mechanics.apply(db, inputs, settings)
        assert report["complete_cards"] == 0
        if back is None:
            assert db.rows("card_mechanic_coverage") == ()
        else:
            assert db.rows("card_mechanic_coverage")[0].values["complete_all"] is False


@pytest.mark.parametrize(
    "raw",
    [
        "{ラストワード}『スティールナイト』5体を出す。それは【合成乙】を持つ。それを好きな枚数アクトしてよい。",
        "{起動}{コスト7}これを{アクト}：『ナイト』5体を出す。それは【合成乙】を持つ。",
        "これがある限り、自分の『ドラゴン』すべては【合成乙】を持つ。",
    ],
)
def test_existing_token_grants_close_without_adding_positive_facts(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, raw: str
) -> None:
    with case(
        tmp_path, monkeypatch, raw, first_batch=True, sections=("7枚引く。",)
    ) as (db, inputs, settings, _):
        with db.transaction():
            report = mechanics.apply(db, inputs, settings)
        assert report["complete_cards"] == 1
        assert {
            r.values["keyword_id"]: r.values["relations"]
            for r in db.rows("mechanic_projection")
        } == {"kw:storm": Json(["grants"])}


@pytest.mark.parametrize(
    "raw",
    [
        "自分のターンごとに1回、自分の他のアミュレットが場を離れたとき、『ホーリータイガー』1体を出す。それは【合成乙】を持つ。",
        "{起動}{コスト2}これを{アクト}：自分のEXエリアの『ドラゴン』1枚を選ぶ。それを場に出す。それは【合成乙】を持つ。【覚醒】状態なら、自分のPPを2回復する。",
    ],
)
def test_unproduced_token_grants_remain_unknown_with_known_sections(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, raw: str
) -> None:
    with case(
        tmp_path, monkeypatch, raw, first_batch=True, sections=("7枚引く。",)
    ) as (db, inputs, settings, _):
        with db.transaction():
            report = mechanics.apply(db, inputs, settings)
        assert report["complete_cards"] == 0
        assert db.rows("mechanic_projection") == ()
        assert db.rows("card_mechanic_coverage") == ()


@pytest.mark.parametrize(
    "section",
    [
        "７枚引く。",
        "『攻撃型ゴーレム』{ウィッチ}ゴーレム・フォロワー{コスト2}{攻撃力}3/{体力}2【突進】【守護】",
        "『ナイト』{ロイヤル}兵士・フォロワー{コスト1}{攻撃力}1/{体力}1自分のエンドフェイズが来たとき、これは消滅する。",
    ],
)
def test_section_proof_does_not_normalize_unknown_structure(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, section: str
) -> None:
    with case(
        tmp_path, monkeypatch, "【合成甲】", first_batch=True, sections=(section,)
    ) as (db, inputs, settings, _):
        with db.transaction():
            report = mechanics.apply(db, inputs, settings)
        assert report["complete_cards"] == 0


@pytest.mark.parametrize(
    "raw",
    [
        _ROSE_FIELD.replace("『薔薇の一撃』の枚数", "『ゴースト』の枚数"),
        _ROSE_FIELD.replace("の枚数」", "の【合成乙】の枚数」"),
        _ROSE_FIELD.split("\n", 1)[0],
        "\n".join(reversed(_ROSE_FIELD.split("\n"))),
    ],
)
def test_named_ex_count_quote_rejects_altered_dependencies(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, raw: str
) -> None:
    with case(
        tmp_path, monkeypatch, raw, first_batch=True, sections=("7枚引く。",)
    ) as (db, inputs, settings, _):
        with db.transaction():
            report = mechanics.apply(db, inputs, settings)
        assert report["complete_cards"] == 0
        assert db.rows("mechanic_projection") == ()


def test_proved_sections_do_not_authorize_cards_outside_the_batch(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    with case(tmp_path, monkeypatch, "7枚引く。", sections=("8枚引く。",)) as (
        db,
        inputs,
        settings,
        _,
    ):
        with db.transaction():
            report = mechanics.apply(db, inputs, settings)
        assert report["complete_cards"] == 0
        assert db.rows("card_mechanic_coverage") == ()


@pytest.mark.parametrize("separator", ["", "\n"], ids=["same-line", "separate-lines"])
def test_grant_followed_by_a_known_body_requires_the_producers_whole_line(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, separator: str
) -> None:
    raw = (
        "{ラストワード}『スティールナイト』5体を出す。それは【合成乙】を持つ。"
        + separator
        + "7枚引く。"
    )
    with case(
        tmp_path, monkeypatch, raw, first_batch=True, sections=("8枚引く。",)
    ) as (db, inputs, settings, _):
        with db.transaction():
            report = mechanics.apply(db, inputs, settings)
        assert report["complete_cards"] == (1 if separator else 0)
        assert {
            r.values["keyword_id"]: r.values["relations"]
            for r in db.rows("mechanic_projection")
        } == ({"kw:storm": Json(["grants"])} if separator else {})


@pytest.mark.parametrize(
    "separator", ["\u2028", "\x1c"], ids=["unicode-line", "file-separator"]
)
@pytest.mark.parametrize("in_section", [False, True], ids=["main", "section"])
def test_non_lf_boundaries_never_separate_provable_clauses(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, separator: str, in_section: bool
) -> None:
    text = "7枚引く。" + separator + "8枚引く。"
    with case(
        tmp_path,
        monkeypatch,
        "【合成甲】" if in_section else text,
        first_batch=True,
        sections=(text if in_section else "9枚引く。",),
    ) as (db, inputs, settings, _):
        with db.transaction():
            report = mechanics.apply(db, inputs, settings)
        assert report["complete_cards"] == 0


@pytest.mark.parametrize(
    "section",
    [
        "『フェアリー』{ロイヤル}妖精・フォロワー{コスト1}{攻撃力}1/{体力}1",
        "『合成トークン』{エルフ}妖精・フォロワー{コスト1}{攻撃力}1/{体力}1",
        "『ヴァイキング』{ロイヤル}盗賊・フォロワー{コスト3}{攻撃力}3/{体力}2【守護】",
    ],
)
def test_token_definition_requires_its_named_class_trait_and_native_suffix(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, section: str
) -> None:
    with case(
        tmp_path, monkeypatch, "【合成甲】", first_batch=True, sections=(section,)
    ) as (db, inputs, settings, _):
        with db.transaction():
            report = mechanics.apply(db, inputs, settings)
        assert report["complete_cards"] == 0
        assert {
            r.values["keyword_id"]: r.values["relations"]
            for r in db.rows("mechanic_projection")
        } == {"kw:ward": Json(["has"])}


@pytest.mark.parametrize(
    "space", [" ", "\u3000"], ids=["ascii-space", "ideographic-space"]
)
@pytest.mark.parametrize("leading", [False, True], ids=["trailing", "leading"])
def test_grant_boundary_keeps_the_producers_exact_untrimmed_line(
    space: str, leading: bool
) -> None:
    line = "{ラストワード}『スティールナイト』5体を出す。それは【合成乙】を持つ。"
    terms = {"kw:storm": "合成乙"}
    assert mechanic_grants.granted_keyword(line, terms) == "kw:storm"
    assert (
        mechanic_grants.granted_keyword(
            space + line if leading else line + space, terms
        )
        is None
    )


@pytest.mark.parametrize(
    "grant", [False, True], ids=["fixed-pp-act", "quick-with-existing-grant"]
)
@pytest.mark.parametrize(
    "positive", [False, True], ids=["no-self-keyword", "self-keyword"]
)
def test_newly_proved_whole_lines_replace_the_previous_bounded_gaps(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, grant: bool, positive: bool
) -> None:
    raw = (
        "{クイック}\n自分のフォロワー1体を選ぶ。それは【合成乙】を持つ。"
        if grant
        else "{起動}{コスト1}これを{アクト}：相手のフォロワー1体を選ぶ。それをアクトする。"
    )
    with case(
        tmp_path,
        monkeypatch,
        ("【合成甲】\n" if positive else "") + raw,
        first_batch=True,
    ) as (db, inputs, settings, _):
        with db.transaction():
            report = mechanics.apply(db, inputs, settings)
        assert report["complete_cards"] == 1
        expected = {}
        if grant:
            expected["kw:storm"] = Json(["grants"])
        if positive:
            expected["kw:ward"] = Json(["has"])
        assert {
            r.values["keyword_id"]: r.values["relations"]
            for r in db.rows("mechanic_projection")
        } == expected


_FINAL_FIELDS = [
    pytest.param(
        "c:0354bd9a205250be958580f4b74b9c44",
        "自分のターンごとに1回、自分の他のアミュレットが場を離れたとき、『ホーリータイガー』1体を出す。それは【守護】を持つ。",
        (
            "『ホーリータイガー』{ビショップ}獣・フォロワー{コスト4}{攻撃力}4/{体力}4【突進】",
        ),
        {"kw:ward": ["grants"]},
        id="BP01-141",
    ),
    pytest.param(
        "c:1c1df3db02625a6688c3a79f1f4628bb",
        "【進化時】下記から1つチョイスする。【1】これは【疾走】を持つ。【2】相手のフォロワー1体を選ぶ。それに「これの攻撃力」と同じダメージ。",
        (),
        {"kw:storm": ["has"]},
        id="BP01-005",
    ),
    pytest.param(
        "c:21fe3e24746457e88fd5f9cef0039c3e",
        "{ファンファーレ}【真紅】状態なら、これは{攻撃力}+1/{体力}+1して、【突進】を持つ。",
        (),
        {"kw:rush": ["has"]},
        id="BP01-112",
    ),
    pytest.param(
        "c:3853b3eeb8845ecab213a36f94376fe6",
        "【突進】【指定攻撃】【守護】\n{ファンファーレ}相手のリーダー1人を選ぶ。それの場のカードが3枚以上なら、これは{攻撃力}+2/{体力}+2して、【オーラ】を持つ。",
        (),
        {
            "kw:aura": ["has"],
            "kw:rush": ["has"],
            "kw:assail": ["has"],
            "kw:ward": ["has"],
        },
        id="BP01-028",
    ),
    pytest.param(
        "c:53097053c3f652b290299c681f47f346",
        "{ファンファーレ}【真紅】状態なら、これは【疾走】を持つ。",
        (),
        {"kw:storm": ["has"]},
        id="BP01-113",
    ),
    pytest.param(
        "c:b2e2e26709a65e08979ba5d0e6d0fb20",
        "{ファンファーレ}自分のフォロワー1体を選ぶ。それは【守護】を持つ。\n{起動}{コスト2}これを{アクト}墓場に置く：【守護】を持つ自分のフォロワー1体を選ぶ。それは{攻撃力}+2/{体力}+2する。",
        (),
        {"kw:ward": ["grants", "refers"]},
        id="BP01-149",
    ),
    pytest.param(
        "c:bcf2ca13d83252188db0e7494b818c61",
        "【必殺】\n{ファンファーレ}リーダーの{体力}-2：これは【突進】を持つ。自分のデッキの上1枚を墓場に置く。",
        (),
        {"kw:rush": ["has"], "kw:bane": ["has"]},
        id="BP01-119",
    ),
    pytest.param(
        "c:d680c6d777c853a5a32e0ca6d2ab2f76",
        "{ファンファーレ}『ドラゴン』1枚をEXエリアに置く。\n{起動}{コスト2}これを{アクト}：自分のEXエリアの『ドラゴン』1枚を選ぶ。それを場に出す。それは【突進】を持つ。【覚醒】状態なら、自分のPPを2回復する。",
        ("『ドラゴン』{ドラゴン}竜族・フォロワー{コスト4}{攻撃力}5/{体力}5",),
        {"kw:rush": ["grants"]},
        id="BP01-078",
    ),
]


@pytest.mark.parametrize(("card_id", "raw", "sections", "expected"), _FINAL_FIELDS)
def test_final_fields_preserve_keyword_ownership_and_complete_all_relations(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    card_id: str,
    raw: str,
    sections: tuple[str, ...],
    expected: dict[str, list[str]],
) -> None:
    with case(
        tmp_path,
        monkeypatch,
        raw,
        first_batch=card_id,
        sections=sections,
    ) as (db, inputs, settings, _):
        support = db.rows("card_engine_support")
        with db.transaction():
            report = mechanics.apply(db, inputs, settings)
        assert report["complete_cards"] == 1
        assert db.rows("card_mechanic_coverage")[0].values["complete_all"] is True
        assert {
            str(row.values["keyword_id"]): row.values["relations"]
            for row in db.rows("mechanic_projection")
        } == {key: Json(list[JsonValue](value)) for key, value in expected.items()}
        assert db.rows("card_engine_support") == support
        assert all(row.values["dsl_id"] is None for row in db.rows("card_mechanic"))


@pytest.mark.parametrize(
    ("card_id", "raw", "sections", "expected"),
    [item for item in _FINAL_FIELDS if item.id != "BP01-149"],
)
@pytest.mark.parametrize("change", ["tail", "quote", "different-owner"])
def test_final_field_proofs_reject_unadmitted_contexts(  # ruff: ignore[too-many-arguments,too-many-positional-arguments] -- each parameter describes an independent source boundary
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    card_id: str,
    raw: str,
    sections: tuple[str, ...],
    expected: dict[str, list[str]],
    change: str,
) -> None:
    if change == "tail":
        raw += "1枚引く。"
    elif change == "quote":
        raw = "これは「\n" + raw + "\n」を持つ。"
    else:
        card_id = "c:00010e129e9e56c5ac66b7b7c597b6e6"
    with case(
        tmp_path,
        monkeypatch,
        raw,
        first_batch=card_id,
        sections=sections,
    ) as (db, inputs, settings, _):
        with db.transaction():
            report = mechanics.apply(db, inputs, settings)
        assert report["complete_cards"] == 0
        projections = {
            str(row.values["keyword_id"]): row.values["relations"]
            for row in db.rows("mechanic_projection")
        }
        # Existing standalone declarations may survive, but no new acquisition may do so.
        acquired = next(key for key in expected if key not in {"kw:bane", "kw:assail"})
        assert acquired not in projections


@pytest.mark.parametrize("suffix", ["【守護】", "自分のPPを1回復する。"])
def test_holy_tiger_definition_needs_its_complete_native_rush_suffix(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    suffix: str,
) -> None:
    raw = "自分のターンごとに1回、自分の他のアミュレットが場を離れたとき、『ホーリータイガー』1体を出す。それは【守護】を持つ。"
    section = (
        "『ホーリータイガー』{ビショップ}獣・フォロワー{コスト4}{攻撃力}4/{体力}4"
        + suffix
    )
    with case(
        tmp_path,
        monkeypatch,
        raw,
        first_batch="c:0354bd9a205250be958580f4b74b9c44",
        sections=(section,),
    ) as (db, inputs, settings, _):
        with db.transaction():
            report = mechanics.apply(db, inputs, settings)
        assert report["complete_cards"] == 0
        assert {
            str(row.values["keyword_id"]): row.values["relations"]
            for row in db.rows("mechanic_projection")
        } == {"kw:ward": Json(["grants"])}


@pytest.mark.parametrize(
    ("card_id", "raw", "_sections", "expected"),
    [item for item in _FINAL_FIELDS if item.id != "BP01-149"],
)
def test_final_field_fact_rejects_cr_and_nonunique_keyword_mapping(
    card_id: str,
    raw: str,
    _sections: tuple[str, ...],
    expected: dict[str, list[str]],
) -> None:
    terms = {
        "kw:ward": "守護",
        "kw:storm": "疾走",
        "kw:rush": "突進",
        "kw:aura": "オーラ",
    }
    assert mechanic_grants.field_fact(card_id, raw, terms) is not None
    assert mechanic_grants.field_fact(card_id, raw + "\r", terms) is None
    # A spelling shared by a second adopted concept cannot identify a unique keyword.
    keyword = next(key for key in expected if key not in {"kw:bane", "kw:assail"})
    assert (
        mechanic_grants.field_fact(card_id, raw, dict(terms, duplicate=terms[keyword]))
        is None
    )
    assert mechanic_grants.field_fact(card_id, raw, {}) is None


def test_final_field_source_bytes_cannot_be_normalized_to_match_the_frozen_owner(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    raw = "{ファンファーレ}【真紅】状態なら、これは【疾走】を持つ。"
    with case(
        tmp_path, monkeypatch, raw, first_batch="c:53097053c3f652b290299c681f47f346"
    ) as (db, inputs, settings, _):
        with db.transaction():
            unit = TextInterner(db).intern(LocalizedText(lang="ja", text=raw + "\r"))
            db.update(
                "face_revision", {"id": "link-revision"}, {"effect_unit_id": unit}
            )
        with pytest.raises(ValueError, match="Card source frozen field differs"):
            with db.transaction():
                mechanics.apply(db, inputs, settings)


def test_reference_field_proof_does_not_close_the_same_text_on_another_card(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    raw = (
        "{ファンファーレ}自分のフォロワー1体を選ぶ。それは【守護】を持つ。\n"
        "{起動}{コスト2}これを{アクト}墓場に置く：【守護】を持つ自分のフォロワー1体を選ぶ。それは{攻撃力}+2/{体力}+2する。"
    )
    with case(
        tmp_path, monkeypatch, raw, first_batch="c:00010e129e9e56c5ac66b7b7c597b6e6"
    ) as (db, inputs, settings, _):
        with db.transaction():
            report = mechanics.apply(db, inputs, settings)
        assert report["complete_cards"] == 0
        assert {
            str(row.values["keyword_id"]): row.values["relations"]
            for row in db.rows("mechanic_projection")
        } == {"kw:ward": Json(["grants", "refers"])}
