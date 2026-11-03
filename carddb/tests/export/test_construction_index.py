"""The construction index keeps exactly the facts a deck validator needs."""

from typing import TYPE_CHECKING

import pytest
from typer.testing import CliRunner

from sve_carddb import cli
from sve_carddb.core.json import array, canonical, digest, integer, object_value, parse
from sve_carddb.export.construction_index import (
    ConstructionIndexError,
    build_construction_index,
)
from sve_carddb.export.preview import Roots, write_preview
from sve_carddb.export.read_api import load_export

from .test_name_annotations import name_snapshot

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue

    from sve_carddb.export.reader import Row, View

MANIFEST_SHA = "sha256:" + "a" * 64


class Pool:
    """The smallest snapshot tables that describe a handful of cards in one region."""

    def __init__(self) -> None:
        vocabulary: list[Row] = [
            {"kind": kind, "code": code}
            for kind, codes in (
                ("class", ("elf", "royal", "neutral")),
                ("title", ("cardfight_vanguard", "umamusume_pretty_derby")),
            )
            for code in codes
        ]
        self.tables: dict[str, list[Row]] = {
            "card": [],
            "face": [],
            "face_revision": [],
            "text_unit": [],
            "rules_name": [],
            "face_rules_name": [],
            "card_mechanic_coverage": [],
            "rules_profile": [
                {
                    "id": "rules-profile:jp:standard",
                    "region": "jp",
                    "format_code": "standard",
                }
            ],
            "restriction": [],
            "vocabulary": vocabulary,
        }
        self.manifest: dict[str, JsonValue] = {
            "data_version": "preview-20261103T000000Z-0001",
            "restriction_coverage": [],
        }

    def name(self, name: str) -> str:
        rules_name_id = f"rn:{name}"
        if not any(row["id"] == rules_name_id for row in self.tables["rules_name"]):
            self.tables["rules_name"].append(
                {"id": rules_name_id, "region": "jp", "official_name": name}
            )
        return rules_name_id

    def card(  # ruff: ignore[too-many-arguments] -- each card fact is one keyword
        self,
        card_id: str,
        name: str,
        *,
        role: str | None = "main",
        class_code: str | None = "elf",
        titles: tuple[str, ...] = (),
        text: str | None = None,
        sections: tuple[str, ...] = (),
        released: bool = True,
        starting_amulet: bool = False,
    ) -> None:
        face, revision = f"f:{card_id}", f"rev:{card_id}"
        units = []
        for ordinal, value in enumerate((text, *sections)):
            unit = None if value is None else f"t:{card_id}:{ordinal}"
            if unit is not None:
                self.tables["text_unit"].append(
                    {"id": unit, "lang": "ja", "text": value}
                )
            units.append(unit)
        self.tables["card"].append(
            {
                "id": card_id,
                "faces": [face],
                "regions": [
                    {
                        "region": "jp",
                        "release_state": "released" if released else "unknown",
                        "deck_role": role,
                    }
                ],
            }
        )
        self.tables["face"].append(
            {
                "id": face,
                "card_id": card_id,
                "current": [{"region": "jp", "revision_id": revision}],
            }
        )
        self.tables["face_revision"].append(
            {
                "id": revision,
                "face_id": face,
                "region": "jp",
                "class_code": class_code,
                "titles": list(titles),
                "effect_unit_id": units[0],
                "sections": [
                    {"ordinal": i, "text_unit_id": unit, "kind": "unknown"}
                    for i, unit in enumerate(units[1:])
                ],
            }
        )
        self.tables["face_rules_name"].append(
            {
                "face_id": face,
                "region": "jp",
                "rules_name_id": self.name(name),
                "role": "primary",
            }
        )
        self.tables["card_mechanic_coverage"].append(
            {
                "card_id": card_id,
                "scope": "shared",
                "printed_mechanisms": [{"code": "starting_amulet", "thresholds": []}]
                if starting_amulet
                else [],
            }
        )

    def build(self) -> JsonValue:
        """The index as a reader sees it after parsing the written bytes."""
        view: View = self.tables
        return parse(
            canonical(build_construction_index(self.manifest, view, "jp", MANIFEST_SHA))
        )


def get(value: JsonValue, *path: str | int) -> JsonValue:
    for key in path:
        value = array(value)[key] if isinstance(key, int) else object_value(value)[key]
    return value


def cards_by_id(index: JsonValue) -> dict[str, dict[str, JsonValue]]:
    columns = object_value(get(index, "cards"))
    return {
        str(card_id): {key: array(values)[i] for key, values in columns.items()}
        for i, card_id in enumerate(array(columns["id"]))
    }


def test_same_name_cards_share_one_code_and_zones_follow_deck_role() -> None:
    pool = Pool()
    pool.card("c:rose-a", "ローズクイーン")
    pool.card("c:rose-b", "ローズクイーン")
    pool.card("c:arisa", "アリサ", role="leader")
    pool.card("c:token", "薔薇の一撃", role="extra")
    pool.card("c:orchis", "決意の人形・オーキス", role="evolve")
    pool.card("c:ep", "EP", role=None, class_code=None)
    pool.card("c:unreleased", "未発売", released=False)
    index = pool.build()
    cards = cards_by_id(index)

    assert get(index, "snapshot") == {
        "data_version": "preview-20261103T000000Z-0001",
        "manifest_sha256": MANIFEST_SHA,
    }
    assert "c:unreleased" not in cards
    assert cards["c:rose-a"]["name"] == cards["c:rose-b"]["name"]
    assert cards["c:rose-a"]["name"] != cards["c:arisa"]["name"]
    assert {
        card: get(index, "zones", integer(cards[card]["zone"])) for card in cards
    } == {
        "c:rose-a": "main",
        "c:rose-b": "main",
        "c:arisa": "leader",
        "c:token": "none",
        "c:orchis": "evolve",
        "c:ep": "none",
    }
    assert cards["c:ep"]["class"] == -1


def test_classes_and_titles_are_dictionary_codes() -> None:
    pool = Pool()
    pool.card(
        "c:carrot",
        "にんじん",
        role="evolve",
        class_code="neutral",
        titles=("umamusume_pretty_derby",),
    )
    pool.card("c:leon", "蒼龍レオン", role="leader", titles=("cardfight_vanguard",))
    index = pool.build()
    cards = cards_by_id(index)
    carrot = cards["c:carrot"]
    assert get(index, "classes", integer(carrot["class"])) == "neutral"
    titles = array(get(index, "titles"))
    assert carrot["titles"] == 1 << titles.index("umamusume_pretty_derby")
    assert get(index, "title_rules") == {"cardfight_vanguard": "vanguard"}


def test_card_text_copy_rules_and_singleton_rule() -> None:
    pool = Pool()
    pool.card(
        "c:onion",
        "オニオン軍団",
        sections=("『オニオン軍団』は自分のデッキに50枚まで入れることができる。",),
    )
    pool.card(
        "c:carrot",
        "にんじん",
        role="evolve",
        sections=("これはエボルヴデッキに10枚まで入れることができる。",),
    )
    pool.card(
        "c:cut",
        "機鋒の咎人・カットスロート",
        class_code="neutral",
        sections=(
            "カード名に『カットスロート』を含まないカードは、メインデッキやエボルヴデッキにそれぞれ1枚までしか入れられない。",
        ),
    )
    pool.card(
        "c:cut-evo", "機鋒の咎人・カットスロート", role="evolve", class_code="neutral"
    )
    pool.card("c:amulet", "士官候補生 エリック", starting_amulet=True)
    index = pool.build()
    cards = cards_by_id(index)
    ids = list(cards)
    copies = [array(c) for c in array(get(index, "copies"))]
    assert sorted((ids[integer(c[0])], c[1], c[2]) for c in copies) == [
        ("c:carrot", "evolve", 10),
        ("c:onion", "main", 50),
    ]
    assert get(index, "singleton_except") == [
        [ids.index("c:cut"), [cards["c:cut"]["name"]]]
    ]
    assert get(index, "start_amulet") == [ids.index("c:amulet")]


@pytest.mark.parametrize(
    "line",
    [
        # An unseen wording that still reads like a copy rule.
        "これはメインデッキに5枚まで入れることができる。",
        # A copy rule naming another card.
        "『ほかのカード』は自分のデッキに50枚まで入れることができる。",
    ],
)
def test_unrecognised_construction_text_stops_the_export(line: str) -> None:
    pool = Pool()
    pool.card("c:odd", "変なカード", sections=(line,))
    with pytest.raises(ConstructionIndexError):
        pool.build()


def test_copy_rule_for_the_wrong_deck_is_rejected() -> None:
    pool = Pool()
    pool.card(
        "c:odd",
        "変なカード",
        sections=("これはエボルヴデッキに10枚まで入れることができる。",),
    )
    with pytest.raises(ConstructionIndexError):
        pool.build()


def test_conflicting_copy_rules_for_one_name_are_rejected() -> None:
    pool = Pool()
    pool.card(
        "c:a",
        "にんじん",
        role="evolve",
        sections=("これはエボルヴデッキに10枚まで入れることができる。",),
    )
    pool.card(
        "c:b",
        "にんじん",
        role="evolve",
        sections=("これはエボルヴデッキに9枚まで入れることができる。",),
    )
    with pytest.raises(ConstructionIndexError):
        pool.build()


def test_restrictions_keep_their_dates_and_skip_rows_that_never_bind() -> None:
    pool = Pool()
    pool.card("c:armor", "星の鎧", class_code="royal")
    armor = pool.name("星の鎧")
    lifted = pool.name("解除済み")
    rows = [
        ("r:banned", "confirmed", "2025-10-01", None, 0),
        ("r:lifted", "confirmed", "2026-09-04", "2026-09-25", 0),
        ("r:withdrawn", "withdrawn", "2026-01-01", None, 1),
        ("r:announced", "announced", "2027-01-01", None, 1),
    ]
    for restriction_id, state, start, until, max_copies in rows:
        pool.tables["restriction"].append(
            {
                "id": restriction_id,
                "profile_id": "rules-profile:jp:standard",
                "effective_from": start,
                "effective_until": until,
                "kind": "copy_limit",
                "state": state,
                "max_copies": max_copies,
                "max_selected_groups": None,
                "members": [
                    {
                        "rules_name_id": lifted
                        if restriction_id == "r:lifted"
                        else armor,
                        "choice_option": 0,
                        "deck_scope": "main",
                    }
                ],
            }
        )
    pool.manifest["restriction_coverage"] = [
        {
            "profile_id": "rules-profile:jp:standard",
            "from_date": "2026-10-26",
            "until_date": "2026-10-27",
            "state": "complete",
        },
        {
            "profile_id": "rules-profile:jp:standard",
            "from_date": "2026-11-01",
            "until_date": None,
            "state": "partial",
        },
    ]
    index = pool.build()
    cards = cards_by_id(index)
    standard = get(index, "formats", "standard")
    assert get(standard, "coverage") == [["2026-10-26", "2026-10-27"]]
    restrictions = [object_value(r) for r in array(get(standard, "restrictions"))]
    summary = [(r["id"], r["from"], r["until"], r["max"]) for r in restrictions]
    assert summary == [
        ("r:banned", "2025-10-01", None, 0),
        ("r:lifted", "2026-09-04", "2026-09-25", 0),
    ]
    armor_code = cards["c:armor"]["name"]
    assert restrictions[0]["members"] == [[armor_code, "main", 0]]
    assert get(restrictions[1], "members", 0, 0) != armor_code


def test_unknown_restriction_state_is_not_guessed() -> None:
    pool = Pool()
    pool.card("c:armor", "星の鎧")
    pool.tables["restriction"].append(
        {
            "id": "r:odd",
            "profile_id": "rules-profile:jp:standard",
            "effective_from": "2025-10-01",
            "effective_until": None,
            "kind": "copy_limit",
            "state": "disputed",
            "max_copies": 0,
            "max_selected_groups": None,
            "members": [
                {
                    "rules_name_id": pool.name("星の鎧"),
                    "choice_option": 0,
                    "deck_scope": "main",
                }
            ],
        }
    )
    with pytest.raises(ConstructionIndexError):
        pool.build()


def test_command_writes_each_region_of_an_export(tmp_path: Path) -> None:
    _projection, snapshot, _plan = name_snapshot()
    roots = Roots(tmp_path / "public", tmp_path / "private")
    write_preview(snapshot, roots, {})
    out = tmp_path / "construction"
    result = CliRunner().invoke(
        cli.app,
        [
            "snapshot",
            "construction-index",
            "--export-dir",
            str(roots.preview),
            "--out-dir",
            str(out),
        ],
    )
    assert result.exit_code == 0, result.output
    report = object_value(parse(result.output.strip().splitlines()[-1].encode()))
    raw = (out / "jp.json").read_bytes()
    assert report == {"jp": {"bytes": len(raw), "sha256": digest(raw)}}
    index = parse(raw)
    assert (
        get(index, "snapshot", "manifest_sha256")
        == load_export(roots.preview).entry["manifest_sha256"]
    )
    assert array(get(index, "cards", "id"))


def test_command_refuses_to_write_inside_the_public_root(tmp_path: Path) -> None:
    _projection, snapshot, _plan = name_snapshot()
    roots = Roots(tmp_path / "public", tmp_path / "private")
    write_preview(snapshot, roots, {})
    result = CliRunner().invoke(
        cli.app,
        [
            "snapshot",
            "construction-index",
            "--export-dir",
            str(roots.preview),
            "--out-dir",
            str(roots.preview / "construction"),
        ],
    )
    assert result.exit_code != 0
    assert not (roots.preview / "construction").exists()
