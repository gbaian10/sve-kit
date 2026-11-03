"""The construction index keeps exactly the facts a deck validator needs."""

from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from typer.testing import CliRunner, Result

from sve_carddb import cli
from sve_carddb.core.json import (
    array,
    canonical,
    digest,
    integer,
    object_value,
    parse,
    string,
)
from sve_carddb.export.construction_index import (
    ConstructionIndexError,
    build_construction_index,
)
from sve_carddb.export.preview import Roots, write_preview
from sve_carddb.export.read_api import load_export
from sve_carddb.export.transport import Batch, Ownership, export_snapshot

from .test_name_annotations import name_snapshot

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.export.reader import Row, View
    from sve_carddb.export.transport import Snapshot

MANIFEST_SHA = "sha256:" + "a" * 64


class Pool:
    """The smallest snapshot tables that describe a handful of cards in one region."""

    def __init__(self, region: str = "jp") -> None:
        self.region = region
        self._names: set[str] = set()
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
                    "id": f"rules-profile:{region}:standard",
                    "region": region,
                    "format_code": "standard",
                }
            ],
            "restriction": [],
            "vocabulary": vocabulary,
        }
        self.manifest: dict[str, JsonValue] = {
            "data_version": "preview-20261103T000000Z-0001",
            "data_as_of": "2026-11-03",
            "restriction_coverage": [],
        }

    def name(self, name: str) -> str:
        rules_name_id = f"rn:{name}"
        if rules_name_id not in self._names:
            self._names.add(rules_name_id)
            self.tables["rules_name"].append(
                {"id": rules_name_id, "region": self.region, "official_name": name}
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
                    {
                        "id": unit,
                        "lang": "ja" if self.region == "jp" else "en",
                        "text": value,
                    }
                )
            units.append(unit)
        self.tables["card"].append(
            {
                "id": card_id,
                "faces": [face],
                "regions": [
                    {
                        "region": self.region,
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
                "current": [{"region": self.region, "revision_id": revision}],
            }
        )
        self.tables["face_revision"].append(
            {
                "id": revision,
                "face_id": face,
                "region": self.region,
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
                "region": self.region,
                "rules_name_id": self.name(name),
                "role": "primary",
            }
        )
        self.tables["card_mechanic_coverage"].append(
            {
                "card_id": card_id,
                "scope": "shared" if self.region == "jp" else "en_override",
                "printed_mechanisms": [{"code": "starting_amulet", "thresholds": []}]
                if starting_amulet
                else [],
                "checked_mechanisms": ["starting_amulet"],
            }
        )

    def build(self) -> JsonValue:
        """The index as a reader sees it after parsing the written bytes."""
        view: View = self.tables
        return parse(
            canonical(
                build_construction_index(self.manifest, view, self.region, MANIFEST_SHA)
            )
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


def construction_snapshot() -> Snapshot:
    projection, _snapshot, _plan = name_snapshot()
    for row in projection.tables["card_mechanic_coverage"]:
        row["checked_mechanisms"] = ["starting_amulet"]
    return export_snapshot(
        projection,
        Ownership(
            {string(row["id"]): "family" for row in projection.tables["printing"]}
        ),
        Batch(
            "preview-20261103T000000Z-0001",
            "2026-11-03T00:00:00Z",
            "2026-11-03",
            ("jp",),
        ),
    )


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


def test_english_copy_and_singleton_sentences() -> None:
    pool = Pool("en")
    pool.card(
        "c:onion",
        "Onion Patch",
        sections=("You can put up to 50 of this card into your deck.",),
    )
    pool.card(
        "c:rapid",
        "Rapid Fire",
        sections=("You can put up to 6 of this card into your deck.",),
    )
    pool.card(
        "c:carrot",
        "Carrot",
        role="evolve",
        sections=("You can put up to 10 of this card into your evolve deck.",),
    )
    pool.card(
        "c:cut",
        "Synthetic Cutthroat",
        sections=(
            'Your main deck and evolve deck may only contain 1 copy of any card without "Cutthroat" in its name.',
        ),
    )
    index = pool.build()
    ids = list(cards_by_id(index))
    copies = [array(row) for row in array(get(index, "copies"))]
    assert [(ids[integer(row[0])], row[1], row[2]) for row in copies] == [
        ("c:carrot", "evolve", 10),
        ("c:onion", "main", 50),
        ("c:rapid", "main", 6),
    ]
    assert get(index, "singleton_except") == [
        [ids.index("c:cut"), [cards_by_id(index)["c:cut"]["name"]]]
    ]


def test_unrecognised_english_copy_sentence_stops_the_export() -> None:
    pool = Pool("en")
    pool.card(
        "c:odd",
        "Synthetic",
        sections=("You can put up to 5 of this card into your main deck.",),
    )
    with pytest.raises(ConstructionIndexError):
        pool.build()


@pytest.mark.parametrize("missing", [False, True])
def test_unknown_starting_amulet_facts_stop_the_export(missing: bool) -> None:
    pool = Pool()
    pool.card("c:unknown", "合成：未確認")
    if missing:
        pool.tables["card_mechanic_coverage"] = []
    else:
        pool.tables["card_mechanic_coverage"][0]["checked_mechanisms"] = []
    with pytest.raises(ConstructionIndexError):
        pool.build()


def test_english_mechanisms_do_not_inherit_shared_facts() -> None:
    pool = Pool("en")
    pool.card("c:amulet", "Synthetic Amulet", starting_amulet=True)
    pool.tables["card_mechanic_coverage"][0]["scope"] = "shared"
    with pytest.raises(ConstructionIndexError):
        pool.build()
    pool.tables["card_mechanic_coverage"].append(
        {
            "card_id": "c:amulet",
            "scope": "en_override",
            "printed_mechanisms": [],
            "checked_mechanisms": ["starting_amulet"],
        }
    )
    assert get(pool.build(), "start_amulet") == []


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


def test_coverage_stops_after_the_snapshots_last_known_day() -> None:
    pool = Pool()
    pool.manifest["data_as_of"] = "2026-11-02"
    pool.manifest["restriction_coverage"] = [
        {
            "profile_id": "rules-profile:jp:standard",
            "from_date": start,
            "until_date": until,
            "state": "complete",
        }
        for start, until in (
            ("2026-10-26", "2026-10-27"),
            ("2026-11-01", None),
            ("2026-11-03", "2026-11-04"),
        )
    ]
    assert get(pool.build(), "formats", "standard", "coverage") == [
        ["2026-10-26", "2026-10-27"],
        ["2026-11-01", "2026-11-03"],
    ]


def test_partial_windows_and_announced_rules_leave_coverage_gaps() -> None:
    pool = Pool()
    pool.manifest["data_as_of"] = "2026-11-06"
    pool.manifest["restriction_coverage"] = [
        {
            "profile_id": "rules-profile:jp:standard",
            "from_date": "2026-11-01",
            "until_date": None,
            "state": "complete",
        },
        {
            "profile_id": "rules-profile:jp:standard",
            "from_date": "2026-11-02",
            "until_date": "2026-11-03",
            "state": "partial",
        },
    ]
    pool.tables["restriction"].append(
        {
            "id": "r:announced",
            "profile_id": "rules-profile:jp:standard",
            "effective_from": "2026-11-04",
            "effective_until": "2026-11-05",
            "kind": "copy_limit",
            "state": "announced",
            "max_copies": 1,
            "max_selected_groups": None,
            "members": [
                {
                    "rules_name_id": pool.name("合成：未確認の制限"),
                    "deck_scope": "main",
                    "choice_option": 0,
                }
            ],
        }
    )
    standard = get(pool.build(), "formats", "standard")
    assert get(standard, "coverage") == [
        ["2026-11-01", "2026-11-02"],
        ["2026-11-03", "2026-11-04"],
        ["2026-11-05", "2026-11-07"],
    ]
    assert get(standard, "restrictions") == []


def _command(export_dir: Path) -> Result:
    return CliRunner().invoke(
        cli.app, ["snapshot", "construction-index", "--export-dir", str(export_dir)]
    )


def test_command_writes_each_region_beside_the_snapshot(tmp_path: Path) -> None:
    snapshot = construction_snapshot()
    roots = Roots(tmp_path / "public", tmp_path / "private")
    write_preview(snapshot, roots, {})
    result = _command(roots.preview)
    assert result.exit_code == 0, result.output
    manifest = string(load_export(roots.preview).entry["manifest_sha256"])
    key = f"construction/{manifest.removeprefix('sha256:')}/jp.json"
    raw = (roots.preview / key).read_bytes()
    report = object_value(parse(result.output.strip().splitlines()[-1].encode()))
    assert report == {key: {"bytes": len(raw), "sha256": digest(raw)}}
    assert get(parse(raw), "snapshot", "manifest_sha256") == manifest
    assert array(get(parse(raw), "cards", "id"))
    # The snapshot closure publish reads is unchanged by the extra files.
    load_export(roots.preview)


def test_command_replaces_output_symlinks_without_modifying_targets(
    tmp_path: Path,
) -> None:
    snapshot = construction_snapshot()
    roots = Roots(tmp_path / "public", tmp_path / "private")
    write_preview(snapshot, roots, {})
    pointer = roots.preview / "snapshots/preview/current.json"
    before = pointer.read_bytes()
    manifest = string(load_export(roots.preview).entry["manifest_sha256"])
    out = roots.preview / "construction" / manifest.removeprefix("sha256:")
    out.mkdir(parents=True)
    (out / "jp.json").symlink_to(pointer)
    result = _command(roots.preview)
    assert result.exit_code == 0, result.output
    assert pointer.read_bytes() == before
    assert not (out / "jp.json").is_symlink()


@pytest.mark.parametrize("level", ["construction", "manifest"])
def test_command_refuses_a_linked_construction_directory(
    tmp_path: Path, level: str
) -> None:
    snapshot = construction_snapshot()
    roots = Roots(tmp_path / "public", tmp_path / "private")
    write_preview(snapshot, roots, {})
    elsewhere = tmp_path / "elsewhere"
    elsewhere.mkdir()
    destination = roots.preview / "construction"
    if level == "manifest":
        destination.mkdir()
        manifest = string(load_export(roots.preview).entry["manifest_sha256"])
        destination /= manifest.removeprefix("sha256:")
    destination.symlink_to(elsewhere)
    assert _command(roots.preview).exit_code != 0
    assert list(elsewhere.iterdir()) == []


REPOSITORY = Path(__file__).resolve().parents[3]
# Kinds that keep a card out of both decks or put it in the evolve deck, in each region's wording.
_NONE_KINDS = ("トークン", "Token", "EP", "Evolution Point", "-")
_EVOLVE_KINDS = ("エボルヴ", "アドバンス", "Evolved", "Advanced")
# The EN page prints Drive Point as a plain Spell; exports take its deck role from the card identity
# shared with JP, which reading pages alone cannot see.
_ROLE_FIXES = {"CP03-127EN": "evolve"}


def _official_pool(region: str) -> Pool:
    """Every reviewed official card of the region, with its printed name, kind and text."""
    pool = Pool(region)
    for path in sorted((REPOSITORY / "card-data" / region).rglob("*.json")):
        if path.name.endswith(".evidence.json"):
            continue
        for value in array(object_value(parse(path.read_bytes()))["cards"]):
            card = object_value(value)
            document = object_value(
                next(iter(object_value(card["documents"]).values()))
            )
            face = object_value(array(object_value(document["record"])["faces"])[0])
            kind = string(
                face["card_type"]
                if region == "jp"
                else object_value(face["info"]).get("Card Type") or "-"
            )
            role = (
                "leader"
                if kind in {"リーダー", "Leader"}
                else None
                if any(part in kind for part in _NONE_KINDS)
                else "evolve"
                if any(part in kind for part in _EVOLVE_KINDS)
                else "main"
            )
            role = _ROLE_FIXES.get(string(card["card_no"]), role)
            pool.card(
                string(card["card_no"]),
                string(face["name"]),
                role=role,
                class_code="neutral",
                text=None if face["text"] is None else string(face["text"]),
                sections=tuple(map(string, array(face["sections"] or []))),
            )
    return pool


@pytest.fixture(scope="session")
def official_construction_indexes() -> dict[str, JsonValue]:
    return {region: _official_pool(region).build() for region in ("jp", "en")}


def test_official_card_text_yields_the_known_construction_rules(
    official_construction_indexes: dict[str, JsonValue],
) -> None:
    for region, suffix, onion in (("jp", "", "BP09-049"), ("en", "EN", "BP09-058EN")):
        index = official_construction_indexes[region]
        cards = cards_by_id(index)
        ids = list(cards)
        copies = {
            (ids[integer(row[0])], row[1], row[2])
            for row in map(array, array(get(index, "copies")))
        }
        assert {
            (onion, "main", 50),
            (f"BP11-045{suffix}", "main", 6),
            (f"CP01-085{suffix}", "evolve", 10),
            (f"CP03-127{suffix}", "evolve", 10),
            (f"ECP01-058{suffix}", "evolve", 10),
        } <= copies, region
        # New construction wording needs review: each region has two main-deck names
        # and seven evolve-deck names with copy rules.
        assert (
            len({(cards[card]["name"], section) for card, section, _ in copies}) == 9
        ), region
        singletons = [array(row) for row in array(get(index, "singleton_except"))]
        holders = {ids[integer(row[0])] for row in singletons}
        assert f"BP19-110{suffix}" in holders, region
        cutthroat = cards[f"BP19-110{suffix}"]["name"]
        assert all(row[1] == [cutthroat] for row in singletons), region
