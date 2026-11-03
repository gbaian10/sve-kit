"""Project one region of a snapshot into the small index a deck validator reads.

The logical fields and rules are specified in docs/sim/deck-construction.md; the wire
layout is in docs/schema/export/construction-index.md.
"""

import re
from collections import defaultdict
from typing import TYPE_CHECKING

from sve_carddb.core.json import array, canonical, digest, integer, object_value, string

if TYPE_CHECKING:
    from collections.abc import Iterable, Mapping
    from pathlib import Path

    from pydantic import JsonValue

    from sve_carddb.export.read_api import Snapshot
    from sve_carddb.export.reader import Row, View

FORMAT = 1
ZONES = ("none", "leader", "main", "evolve")
_ZONE_OF_ROLE = {"leader": 1, "main": 2, "evolve": 3}
# CR 14: titles whose decks have construction rules of their own.
_TITLE_RULES = {"cardfight_vanguard": "vanguard"}

# Card text that changes construction (CR 6.1.2), per region. A sentence that looks like a
# copy rule but matches no template stops the export rather than being silently ignored.
_COPIES = {
    "jp": (
        (
            re.compile(
                r"『(?P<name>[^』]+)』は自分のデッキに(?P<n>\d+)枚まで入れることができる。"
            ),
            "main",
        ),
        (
            re.compile(r"これはエボルヴデッキに(?P<n>\d+)枚まで入れることができる。"),
            "evolve",
        ),
    ),
    "en": (
        (
            re.compile(r"You can put up to (?P<n>\d+) of this card into your deck\."),
            "main",
        ),
        (
            re.compile(
                r"You can put up to (?P<n>\d+) of this card into your evolve deck\."
            ),
            "evolve",
        ),
    ),
}
_SINGLETON = {
    "jp": re.compile(
        r"カード名に『(?P<part>[^』]+)』を含まないカードは、"
        r"メインデッキやエボルヴデッキにそれぞれ1枚までしか入れられない。"
    ),
    "en": re.compile(
        r"Your main deck and evolve deck may only contain 1 copy of any card without "
        r"\"(?P<part>[^\"]+)\" in its name\."
    ),
}
_LOOKS_LIKE_RULE = {
    "jp": re.compile(r"枚まで入れ|枚までしか入れ"),
    "en": re.compile(r"up to \d+ of this card|may only contain \d+ cop"),
}


class ConstructionIndexError(ValueError):
    """The snapshot cannot be projected without guessing a construction fact."""


def build_construction_index(
    manifest: Mapping[str, JsonValue], tables: View, region: str, manifest_sha256: str
) -> dict[str, JsonValue]:
    """The region's construction index, bound to the snapshot it was read from."""
    if region not in _COPIES:
        raise ConstructionIndexError(f"Unsupported region {region}")
    names = _Names(tables, region)
    titles = sorted(_vocabulary(tables, "title"))
    cards = _Cards(tables, region, names, titles)
    copies, singletons = _text_rules(cards.rule_texts, region, names)
    return {
        "construction_index_format": FORMAT,
        "region": region,
        "snapshot": {
            "data_version": manifest["data_version"],
            "manifest_sha256": manifest_sha256,
        },
        "classes": list(cards.classes),
        "titles": list(titles),
        "title_rules": {k: v for k, v in _TITLE_RULES.items() if k in titles},
        "zones": list(ZONES),
        "names": names.count(),
        "cards": cards.columns(),
        "copies": copies,
        "singleton_except": singletons,
        "start_amulet": list(cards.amulets),
        "formats": _formats(manifest, tables, region, names),
    }


def write_construction_indexes(
    snapshot: Snapshot, out_dir: Path
) -> dict[str, JsonValue]:
    """Write `<region>.json` for every region of the snapshot; report each file's hash."""
    out_dir.mkdir(parents=True, exist_ok=True)
    report: dict[str, JsonValue] = {}
    for region in map(string, array(snapshot.manifest["regions"])):
        raw = canonical(
            build_construction_index(
                snapshot.manifest, snapshot.tables, region, snapshot.manifest_sha256
            )
        )
        (out_dir / f"{region}.json").write_bytes(raw)
        report[region] = {"bytes": len(raw), "sha256": digest(raw)}
    return report


class _Cards:
    """Columns of the region's released cards, in card id order."""

    def __init__(
        self, tables: View, region: str, names: _Names, titles: list[str]
    ) -> None:
        faces = {string(row["id"]): row for row in tables["face"]}
        revisions = {string(row["id"]): row for row in tables["face_revision"]}
        texts = {string(row["id"]): string(row["text"]) for row in tables["text_unit"]}
        starting_amulet = _starting_amulets(tables["card_mechanic_coverage"], region)
        self.classes = sorted(_vocabulary(tables, "class"))
        self.ids: list[JsonValue] = []
        self.names: list[JsonValue] = []
        self.class_codes: list[JsonValue] = []
        self.title_bits: list[JsonValue] = []
        self.zones: list[JsonValue] = []
        self.amulets: list[int] = []
        self.rule_texts: list[tuple[int, int, int, list[str]]] = []
        released = sorted(
            (string(card["id"]), card, view)
            for card in tables["card"]
            for view in map(object_value, array(card["regions"]))
            if view["region"] == region and view["release_state"] == "released"
        )
        for position, (card_id, card, view) in enumerate(released):
            front = faces[string(array(card["faces"])[0])]
            revision = revisions[_current(front, region)]
            name = names.code_of_face(string(front["id"]))
            zone = _ZONE_OF_ROLE.get(string(view["deck_role"] or ""), 0)
            self.ids.append(card_id)
            self.names.append(name)
            self.class_codes.append(self._class(revision["class_code"], zone, card_id))
            self.title_bits.append(
                sum(1 << titles.index(string(t)) for t in array(revision["titles"]))
            )
            self.zones.append(zone)
            if card_id in starting_amulet:
                self.amulets.append(position)
            lines = _effect_lines(revision, texts)
            if lines:
                self.rule_texts.append((position, zone, name, lines))

    def _class(self, code: JsonValue, zone: int, card_id: str) -> int:
        if code is not None:
            return self.classes.index(string(code))
        # Only cards that never enter a deck (EP, SEP) print no class.
        if zone != 0:
            raise ConstructionIndexError(
                f"Card {card_id} goes in a deck without a class"
            )
        return -1

    def columns(self) -> dict[str, JsonValue]:
        return {
            "id": self.ids,
            "name": self.names,
            "class": self.class_codes,
            "titles": self.title_bits,
            "zone": self.zones,
        }


class _Names:
    """Integer codes for the region's rules names, in id order."""

    def __init__(self, tables: View, region: str) -> None:
        rows = [row for row in tables["rules_name"] if row["region"] == region]
        rows.sort(key=lambda row: string(row["id"]))
        self._code = {string(row["id"]): code for code, row in enumerate(rows)}
        self.official = [string(row["official_name"]) for row in rows]
        bound: dict[str, list[str]] = defaultdict(list)
        for row in tables["face_rules_name"]:
            if row["region"] == region and row["role"] == "primary":
                bound[string(row["face_id"])].append(string(row["rules_name_id"]))
        self._face = bound

    def code(self, rules_name_id: str) -> int:
        try:
            return self._code[rules_name_id]
        except KeyError:
            raise ConstructionIndexError(
                f"Unknown rules name {rules_name_id}"
            ) from None

    def code_of_face(self, face_id: str) -> int:
        """CR 6.1.1.4.1: the front face's name counts, so a card has exactly one."""
        bound = self._face.get(face_id, [])
        if len(bound) != 1:
            raise ConstructionIndexError(f"Face {face_id} has {len(bound)} rules names")
        return self.code(bound[0])

    def containing(self, part: str) -> list[int]:
        return [code for code, name in enumerate(self.official) if part in name]

    def count(self) -> int:
        return len(self.official)


def _vocabulary(tables: View, kind: str) -> set[str]:
    return {string(row["code"]) for row in tables["vocabulary"] if row["kind"] == kind}


def _current(face: Row, region: str) -> str:
    for entry in map(object_value, array(face["current"])):
        if entry["region"] == region:
            return string(entry["revision_id"])
    raise ConstructionIndexError(f"Face {face['id']} has no current {region} wording")


def _starting_amulets(rows: Iterable[Row], region: str) -> set[str]:
    """The region's coverage row wins over the shared one, as in the snapshot contract."""
    found: dict[str, bool] = {}
    for scope in ("shared", region + "_override"):
        for row in rows:
            if row["scope"] == scope:
                found[string(row["card_id"])] = any(
                    object_value(m)["code"] == "starting_amulet"
                    for m in array(row["printed_mechanisms"] or [])
                )
    return {card for card, has in found.items() if has}


def _effect_lines(revision: Row, texts: Mapping[str, str]) -> list[str]:
    units = [revision["effect_unit_id"]] + [
        object_value(section)["text_unit_id"]
        for section in array(revision["sections"] or [])
    ]
    return [
        line
        for unit in units
        if unit is not None
        for line in texts[string(unit)].split("\n")
    ]


def _text_rules(
    cards: Iterable[tuple[int, int, int, list[str]]], region: str, names: _Names
) -> tuple[list[JsonValue], list[JsonValue]]:
    limits: dict[tuple[int, str], int] = {}
    copies: list[JsonValue] = []
    singletons: list[JsonValue] = []
    for position, zone, name, lines in cards:
        for line in (line.strip() for line in lines):
            if not _LOOKS_LIKE_RULE[region].search(line):
                continue
            singleton = _SINGLETON[region].fullmatch(line)
            if singleton:
                exempt: list[JsonValue] = list(names.containing(singleton["part"]))
                singletons.append([position, exempt])
                continue
            section, limit = _copy_rule(line, region, names, name)
            if ZONES[zone] != section:
                raise ConstructionIndexError(
                    f"Card {position} has a {section} copy rule but goes to {ZONES[zone]}"
                )
            # The validator takes one limit per name and section (spec §3.1).
            if limits.setdefault((name, section), limit) != limit:
                raise ConstructionIndexError(f"Conflicting copy rules for name {name}")
            copies.append([position, section, limit])
    return copies, singletons


def _copy_rule(line: str, region: str, names: _Names, own: int) -> tuple[str, int]:
    for pattern, section in _COPIES[region]:
        match = pattern.fullmatch(line)
        if match is None:
            continue
        named = match.groupdict().get("name")
        if named is not None and names.official[own] != named:
            raise ConstructionIndexError("A copy rule names another card")
        return section, int(match["n"])
    raise ConstructionIndexError("Unrecognised construction rule in card text")


def _formats(
    manifest: Mapping[str, JsonValue], tables: View, region: str, names: _Names
) -> dict[str, JsonValue]:
    formats: dict[str, JsonValue] = {}
    windows = [object_value(w) for w in array(manifest["restriction_coverage"] or [])]
    for profile in tables["rules_profile"]:
        if profile["region"] != region:
            continue
        profile_id = string(profile["id"])
        coverage: list[JsonValue] = [
            [w["from_date"], w["until_date"]]
            for w in windows
            if w["profile_id"] == profile_id and w["state"] == "complete"
        ]
        restrictions: list[JsonValue] = []
        for row in sorted(tables["restriction"], key=lambda r: string(r["id"])):
            # Only confirmed rows apply; announced and withdrawn ones never bind a deck.
            if row["profile_id"] != profile_id or row["state"] in {
                "announced",
                "withdrawn",
            }:
                continue
            if row["state"] != "confirmed" or row["kind"] not in {
                "copy_limit",
                "choice_group",
            }:
                raise ConstructionIndexError(
                    f"Restriction {row['id']} is {row['state']} {row['kind']}"
                )
            members = [object_value(m) for m in array(row["members"])]
            restrictions.append(
                {
                    "id": row["id"],
                    "kind": row["kind"],
                    "from": row["effective_from"],
                    "until": row["effective_until"],
                    "max": row["max_copies"],
                    "max_groups": row["max_selected_groups"],
                    "members": [
                        [
                            names.code(string(m["rules_name_id"])),
                            string(m["deck_scope"]),
                            integer(m["choice_option"]),
                        ]
                        for m in members
                    ],
                }
            )
        formats[string(profile["format_code"])] = {
            "coverage": coverage,
            "restrictions": restrictions,
        }
    return formats
