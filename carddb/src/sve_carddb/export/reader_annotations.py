"""Public annotation closure and source relationships checked independently of the producer."""

from itertools import pairwise
from typing import TYPE_CHECKING, Never

from sve_carddb.contracts.four_layer import hash_payload
from sve_carddb.core.json import array, canonical, digest, integer, object_value, string
from sve_carddb.export.text_owners import TextOwner, TextOwners

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.export.reader import Row, View


def _fail(reason: str) -> Never:
    raise ValueError("public-annotation/" + reason)


def _index(rows: list[Row], fields: tuple[str, ...]) -> dict[bytes, Row]:
    result: dict[bytes, Row] = {}
    for row in rows:
        key = canonical([row[field] for field in fields])
        if key in result:
            _fail("duplicate_key")
        result[key] = row
    return result


def _get(index: dict[bytes, Row], key: list[JsonValue]) -> Row:
    encoded = canonical(key)
    if encoded not in index:
        _fail("reference")
    return index[encoded]


def _ordered(values: list[JsonValue]) -> None:
    keys = [canonical(v) for v in values]
    if keys != sorted(set(keys)):
        _fail("ordering")


def _ranges(text: str, occurrences: list[Row]) -> None:
    all_ranges: list[tuple[int, int]] = []
    order: list[tuple[int, int, bytes]] = []
    for ordinal, occurrence in enumerate(occurrences):
        if occurrence["ordinal"] != ordinal or type(occurrence["ordinal"]) is not int:
            _fail("ordering")
        spans = [object_value(r) for r in array(occurrence["ranges"])]
        if not spans:
            _fail("range")
        ranges = []
        for span in spans:
            try:
                start, end = integer(span["start"]), integer(span["end"])
            except ValueError:
                _fail("range")
            if not 0 <= start < end <= len(text):
                _fail("range")
            ranges.append((start, end))
        if ranges != sorted(ranges) or any(a[1] > b[0] for a, b in pairwise(ranges)):
            _fail("range")
        all_ranges.extend(ranges)
        order.append((ranges[0][0], ranges[-1][1], canonical(occurrence["reference"])))
    if order != sorted(order):
        _fail("ordering")
    all_ranges.sort()
    if any(a[1] > b[0] for a, b in pairwise(all_ranges)):
        _fail("range")


class _Closure:
    def __init__(self, view: View, languages: tuple[str, ...]) -> None:
        self.view = view
        self.languages = languages
        self.owners = TextOwners(view)
        self.texts = _index(view["text_unit"], ("id",))
        self.sets = _index(view["annotation_set"], ("id",))
        self.translations = _index(view["translation"], ("id",))
        self.fields = _index(view["field_annotation"], ("owner", "field", "ordinal"))
        self.concepts = _index(view["annotation_concept"], ("id",))
        self.vocabulary = _index(view["vocabulary"], ("kind", "code"))
        self.used_sets: set[bytes] = set()
        self.used_translations: set[bytes] = set()
        self.used_concepts: set[bytes] = set()
        self.bold: dict[bytes, JsonValue] = {}

    def text(self, identifier: JsonValue) -> Row:
        """Always resolve exact text identity before exposing a semantic range."""
        return _get(self.texts, [identifier])

    def pointer(self, pointer: Row) -> str:
        """Missing owners and invalid fields remain owner failures, not missing translations."""
        try:
            return self.owners.pointer(pointer)
        except KeyError, ValueError, TypeError:
            _fail("owner")

    def owner(self, owner: Row) -> TextOwner:
        """All pointer owners must resolve through the same public key boundary."""
        try:
            return self.owners.get(owner)
        except KeyError, ValueError, TypeError:
            _fail("owner")

    def original_set(self, pointer: Row) -> JsonValue:
        """The complete reader can treat a verified absent field row as empty annotation."""
        key = canonical([pointer[name] for name in ("owner", "field", "ordinal")])
        return self.fields[key]["annotation_set_id"] if key in self.fields else None

    def set_for_text(self, identifier: JsonValue, unit: JsonValue) -> None:
        """An explicit set can never be borrowed merely because another text has equal length."""
        if identifier is None:
            return
        row = _get(self.sets, [identifier])
        if row["text_unit_id"] != unit:
            _fail("text_identity")
        self.used_sets.add(canonical([identifier]))


def _concepts(closure: _Closure) -> None:
    cards = {row["id"] for row in closure.view["card"]}
    targets = {
        (table, string(row["id"])): row
        for table in ("keyword", "cr_clause", "ruling_revision")
        for row in closure.view[table]
    }
    for row in closure.concepts.values():
        if row["category"] not in {
            "card_name",
            "rule_term",
            "trait",
            "keyword",
            "ability",
        }:
            _fail("enum")
        _ordered(array(row["card_ids"]))
        _ordered(array(row["explanations"]))
        if any(identifier not in cards for identifier in array(row["card_ids"])) or (
            row["category"] != "card_name" and array(row["card_ids"])
        ):
            _fail("reference")
        for raw in array(row["explanations"]):
            reference = object_value(raw)
            key = string(reference["kind"]), string(reference["id"])
            if key not in targets:
                _fail("reference")
            unit = targets[key][
                {
                    "keyword": "definition_unit_id",
                    "cr_clause": "text_unit_id",
                    "ruling_revision": "decision_unit_id",
                }[key[0]]
            ]
            if unit is None:
                _fail("reference")
            closure.text(unit)


def _reference(closure: _Closure, occurrence: Row) -> None:
    reference = object_value(occurrence["reference"])
    kind = string(reference["kind"])
    fixed_bold = False
    if kind == "vocabulary":
        key = array(reference["key"])
        _get(closure.vocabulary, key)
        fixed_bold = key[0] in {"class", "type"}
    else:
        identifier = reference["term_id" if kind == "card_name" else "key"]
        concept = _get(closure.concepts, [identifier])
        if (kind == "card_name") != (concept["category"] == "card_name"):
            _fail("reference")
        closure.used_concepts.add(canonical([identifier]))
        fixed_bold = concept["category"] != "rule_term"
    if fixed_bold and occurrence["bold"] is not True:
        _fail("reference")
    bold_key = canonical(reference)
    if bold_key in closure.bold and closure.bold[bold_key] != occurrence["bold"]:
        _fail("reference")
    closure.bold[bold_key] = occurrence["bold"]


def _sets(closure: _Closure) -> None:
    languages = closure.languages
    for text in closure.texts.values():
        if text["lang"] not in languages:
            _fail("reference")
        if (
            text["id"]
            != "t:"
            + string(text["lang"])
            + ":"
            + digest(string(text["text"]).encode())[7:23]
        ):
            _fail("identity")
    for row in closure.sets.values():
        exact_text = string(closure.text(row["text_unit_id"])["text"])
        occurrences = [object_value(o) for o in array(row["occurrences"])]
        if not occurrences:
            _fail("range")
        _ranges(exact_text, occurrences)
        for occurrence in occurrences:
            _reference(closure, occurrence)
        if row["id"] != "ann:" + hash_payload(
            {
                "recipe": "annotation-v1",
                "text_unit_id": row["text_unit_id"],
                "occurrences": occurrences,
            }
        ):
            _fail("identity")
    for row in closure.fields.values():
        pointer = {name: row[name] for name in ("owner", "field", "ordinal")}
        unit = closure.pointer(pointer)
        closure.text(unit)
        closure.set_for_text(row["annotation_set_id"], unit)


def _same_face(left: TextOwner, right: TextOwner) -> None:
    if (
        left.owner["kind"] != right.owner["kind"]
        or left.card_id is None
        or left.card_id != right.card_id
        or left.face_id != right.face_id
    ):
        _fail("owner")


def _jp(closure: _Closure, receiver: TextOwner, value: Row, translation: Row) -> None:
    source = closure.owner(object_value(object_value(value["source"])["owner"]))
    _same_face(receiver, source)
    if (
        receiver.region != "en"
        or source.region != "jp"
        or receiver.mapping_state != "confirmed"
        or source.mapping_state != "confirmed"
    ):
        _fail("basis")
    if (
        value["field"] not in {"name", "effect", "flavor"}
        or value["ordinal"] is not None
        or value["target_lang"] != "zh-Hant"
        or value["counterpart"] is not None
    ):
        _fail("basis")
    if (
        object_value(value["source"])["field"] != value["field"]
        or object_value(value["source"])["ordinal"] is not None
    ):
        _fail("owner")
    allowed = translation["authority"] == "unofficial" and translation["origin"] in {
        "project",
        "machine",
    }
    name_official = (
        value["field"] == "name"
        and translation["authority"] == "digital_official"
        and translation["origin"] == "official"
    )
    if not (allowed or name_official):
        _fail("basis")


def _counterpart(
    closure: _Closure, receiver: TextOwner, value: Row, translation: Row
) -> None:
    if (
        value["counterpart"] is None
        or translation["origin"] != "official"
        or translation["authority"] != "sve_official"
        or value["target_lang"] not in {"ja", "en"}
    ):
        _fail("basis")
    pointer = object_value(value["counterpart"])
    unit = closure.pointer(pointer)
    donor = closure.owner(object_value(pointer["owner"]))
    _same_face(receiver, donor)
    if (
        receiver.region == donor.region
        or donor.region not in {"jp", "en"}
        or receiver.region not in {"jp", "en"}
    ):
        _fail("basis")
    if pointer["field"] != value["field"]:
        _fail("owner")
    if (
        closure.text(unit)["lang"] != value["target_lang"]
        or unit != translation["text_unit_id"]
        or closure.original_set(pointer) != translation["annotation_set_id"]
    ):
        _fail("text_identity")


def _selection(closure: _Closure, receiver: TextOwner, value: Row) -> None:
    translation = _get(closure.translations, [value["translation_id"]])
    closure.used_translations.add(canonical([value["translation_id"]]))
    source = object_value(value["source"])
    receiver_pointer: Row = {
        "owner": receiver.owner,
        "field": value["field"],
        "ordinal": value["ordinal"],
    }
    basis = value["basis"]
    if basis == "jp_source":
        _jp(closure, receiver, value, translation)
    elif basis in {"own_source", "official_counterpart"}:
        if basis == "official_counterpart":
            _counterpart(closure, receiver, value, translation)
        if source != receiver_pointer:
            _fail("owner")
        if basis == "own_source" and value["counterpart"] is not None:
            _fail("basis")
    else:
        _fail("basis")
    closure.pointer(receiver_pointer)
    if closure.pointer(source) != translation["source_unit_id"]:
        _fail("text_identity")
    if (
        basis == "jp_source"
        and closure.text(translation["source_unit_id"])["lang"] != "ja"
    ):
        _fail("basis")
    if (
        value["target_lang"] != translation["target_lang"]
        or closure.text(translation["text_unit_id"])["lang"] != value["target_lang"]
    ):
        _fail("text_identity")
    closure.set_for_text(translation["annotation_set_id"], translation["text_unit_id"])


def validate_annotations(view: View, languages: tuple[str, ...]) -> None:
    """Full closure verification runs before any consumer can obtain a ready text view."""
    closure = _Closure(view, languages)
    _concepts(closure)
    _sets(closure)
    for receiver in closure.owners.by_owner.values():
        values = [object_value(v) for v in array(receiver.row["translations"])]
        _index(values, ("field", "ordinal", "target_lang"))
        for value in values:
            _selection(closure, receiver, value)
    if (
        closure.used_sets != closure.sets.keys()
        or closure.used_translations != closure.translations.keys()
        or closure.used_concepts != closure.concepts.keys()
    ):
        _fail("reference")
