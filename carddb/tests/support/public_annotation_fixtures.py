"""Independent public scenarios adapt declared relationships into reader component rows."""

from copy import deepcopy
from pathlib import Path

from pydantic import JsonValue, TypeAdapter

from sve_carddb.core.json import array, canonical, digest, integer, object_value, string

ROOT = Path(__file__).resolve().parents[3] / "docs/schema/export"
CASES = object_value(
    TypeAdapter(JsonValue).validate_json(
        (ROOT / "public-annotation-cases.json").read_bytes()
    )
)
DEFINITIONS = object_value(
    object_value(
        TypeAdapter(JsonValue).validate_json(
            (ROOT / "public-annotation.schema.json").read_bytes()
        )
    )["$defs"]
)


def mutate(
    value: JsonValue, changes: dict[str, JsonValue], remove: list[JsonValue]
) -> JsonValue:
    """Mutation never sorts occurrences, repairs references or recomputes a broken identity."""
    result = deepcopy(value)
    for pointer, replacement in changes.items():
        if not pointer:
            result = deepcopy(replacement)
            continue
        current, last = _parent(result, pointer)
        if isinstance(current, list):
            current[int(last)] = deepcopy(replacement)
        else:
            object_value(current)[last] = deepcopy(replacement)
    for removed in remove:
        current, last = _parent(result, string(removed))
        if isinstance(current, list):
            del current[int(last)]
        else:
            del object_value(current)[last]
    return result


def _parent(value: JsonValue, pointer: str) -> tuple[JsonValue, str]:
    parts = [p.replace("~1", "/").replace("~0", "~") for p in pointer.split("/")[1:]]
    current = value
    for part in parts[:-1]:
        current = (
            array(current)[int(part)]
            if isinstance(current, list)
            else object_value(current)[part]
        )
    return current, parts[-1]


def fixture(name: str) -> JsonValue:
    """Only the documented direct overlay on jp_en is expanded."""
    value = deepcopy(object_value(CASES["fixtures"])[name])
    if isinstance(value, dict) and "fixture" in value:
        base = fixture(string(value["fixture"]))
        return _prune_scaffolding(
            base,
            mutate(
                base,
                object_value(value.get("changes", {})),
                array(value.get("remove", [])),
            ),
        )
    return value


def inputs(case: dict[str, JsonValue]) -> list[JsonValue]:
    """Each variant starts from an independent copy of the completed input mutation."""
    params = object_value(case["input"])
    base = fixture(string(params["fixture"])) if "fixture" in params else params
    result = _prune_scaffolding(
        base,
        mutate(
            base,
            object_value(params.get("changes", {})),
            array(params.get("remove", [])),
        ),
    )
    return [
        _prune_scaffolding(
            result,
            mutate(
                result,
                object_value(object_value(v).get("changes", {})),
                array(object_value(v).get("remove", [])),
            ),
        )
        for v in array(params.get("variants", [{}]))
    ]


def _prune_scaffolding(before: JsonValue, after: JsonValue) -> JsonValue:
    if (
        not isinstance(before, dict)
        or not isinstance(after, dict)
        or "annotation_sets" not in before
    ):
        return after
    previous = _used_sets(before)
    current = _used_sets(after)
    after["annotation_sets"] = [
        r
        for r in array(after["annotation_sets"])
        if array(r)[0] not in previous - current
    ]
    old_concepts = _used_concepts(before)
    new_concepts = _used_concepts(after)
    after["concepts"] = [
        r
        for r in array(after["concepts"])
        if array(r)[0] not in old_concepts - new_concepts
    ]
    return after


def _used_sets(value: dict[str, JsonValue]) -> set[str]:
    return {string(array(r)[3]) for r in array(value["field_annotations"])} | {
        string(array(r)[7])
        for r in array(value["translations"])
        if array(r)[7] is not None
    }


def _used_concepts(value: dict[str, JsonValue]) -> set[str]:
    result = set()
    for row in array(value["annotation_sets"]):
        for occurrence in array(array(row)[2]):
            reference = object_value(array(occurrence)[1])
            if reference["kind"] != "vocabulary":
                result.add(
                    string(
                        reference[
                            "term_id" if reference["kind"] == "card_name" else "key"
                        ]
                    )
                )
    return result


def decode(name: str, value: JsonValue) -> dict[str, JsonValue]:
    """Only the fixed schema determines tuple columns and nested tuple types."""
    definition = object_value(DEFINITIONS[name])
    return {
        string(column): _decode(object_value(kind), cell)
        for column, kind, cell in zip(
            array(definition["x-columns"]),
            array(definition["x-types"]),
            array(value),
            strict=True,
        )
    }


def _decode(kind: dict[str, JsonValue], value: JsonValue) -> JsonValue:
    if "nullable" in kind:
        return None if value is None else _decode(object_value(kind["nullable"]), value)
    if "array" in kind:
        return [_decode(object_value(kind["array"]), v) for v in array(value)]
    if "ref" in kind:
        return decode(string(kind["ref"]), value)
    return deepcopy(value)


def owner_row(owner: dict[str, JsonValue], fields: JsonValue) -> dict[str, JsonValue]:
    """Unlisted originals stay null; array entries come only from declared fixture fields."""
    kind = string(owner["kind"])
    columns = {
        "face_revision": {"name": "name_unit_id", "effect": "effect_unit_id"},
        "printing_face": {
            "name": "printed_name_unit_id",
            "effect": "printed_effect_unit_id",
            "flavor": "flavor_unit_id",
        },
        "qa_version": {"question": "question_unit_id", "answer": "answer_unit_id"},
        "cr_clause": {"effect": "text_unit_id"},
        "vocabulary": {"label": "label_unit_id"},
        "product": {"label": "name_unit_id"},
        "product_family": {"label": "name_unit_id"},
        "keyword": {"label": "name_unit_id", "effect": "definition_unit_id"},
    }
    result: dict[str, JsonValue] = dict.fromkeys(columns[kind].values())
    result |= {"translations": [], "sections": [], "actions": []}
    for raw in array(fields):
        name, ordinal, unit = array(raw)
        if name == "section":
            array(result["sections"]).append({"ordinal": ordinal, "text_unit_id": unit})
        elif name == "action_label":
            assert integer(ordinal) == len(array(result["actions"]))
            array(result["actions"]).append({"label_unit_id": unit})
        else:
            result[columns[kind][string(name)]] = unit
    return result


def view(value: dict[str, JsonValue]) -> dict[str, list[dict[str, JsonValue]]]:
    """Build only public relationships from owner metadata; producer flags are never copied."""
    result: dict[str, list[dict[str, JsonValue]]] = {
        name: []
        for name in (
            "card",
            "face",
            "face_revision",
            "printing",
            "qa_version",
            "cr_clause",
            "product",
            "product_family",
            "keyword",
            "vocabulary",
            "ruling_revision",
            "text_unit",
            "annotation_set",
            "field_annotation",
            "translation",
            "annotation_concept",
        )
    }
    result["text_unit"] = [
        dict(zip(("id", "lang", "text"), array(r), strict=True))
        for r in array(value["text_units"])
    ]
    for table, key in (
        ("annotation_set", "annotation_sets"),
        ("field_annotation", "field_annotations"),
        ("translation", "translations"),
        ("annotation_concept", "concepts"),
    ):
        result[table] = [decode(table, r) for r in array(value[key])]
    result["vocabulary"] = [
        dict(
            zip(
                ("kind", "code", "label_unit_id", "active", "translations"),
                array(r),
                strict=True,
            )
        )
        for r in array(value["vocabulary"])
    ]
    cards = {string(c): {"id": c, "regions": []} for c in array(value["cards"])}
    result["card"] = list(cards.values())
    owners: dict[bytes, dict[str, JsonValue]] = {}
    translated: dict[bytes, dict[str, JsonValue]] = {}
    faces: dict[str, dict[str, JsonValue]] = {}
    for raw in array(value["owners"]):
        info = object_value(raw)
        owner = object_value(info["owner"])
        kind = string(owner["kind"])
        row = owner_row(owner, info["fields"])
        public_owner = deepcopy(owner)
        if kind in {"face_revision", "printing_face"}:
            public_owner = _card_owner(info, row, result, cards, faces)
        elif kind == "vocabulary":
            matching = [
                r
                for r in result[kind]
                if r["kind"] == owner["vocabulary_kind"] and r["code"] == owner["code"]
            ]
            if not matching:
                continue
            row = matching[0]
        else:
            row["id"] = owner["id"]
            result[kind].append(row)
        owners[canonical(owner)] = row
        translated[canonical(owner)] = public_owner
    result["face"] = list(faces.values())
    for item in result["field_annotation"]:
        item["owner"] = translated.get(canonical(item["owner"]), item["owner"])
    for raw in array(value["field_translations"]):
        entry = object_value(raw)
        receiver = decode("PublicTextPointer", entry["receiver"])
        row = owners[canonical(receiver["owner"])]
        selection = decode("FieldTranslation", entry["value"])
        for name in ("source", "counterpart"):
            if selection[name] is not None:
                pointer = object_value(selection[name])
                pointer["owner"] = translated.get(
                    canonical(pointer["owner"]), pointer["owner"]
                )
        array(row["translations"]).append(selection)
    return result


def _card_owner(
    info: dict[str, JsonValue],
    row: dict[str, JsonValue],
    result: dict[str, list[dict[str, JsonValue]]],
    cards: dict[str, dict[str, JsonValue]],
    faces: dict[str, dict[str, JsonValue]],
) -> dict[str, JsonValue]:
    owner = object_value(info["owner"])
    public_owner = deepcopy(owner)
    kind = string(owner["kind"])
    card_id, region = string(info["card_id"]), string(info["region"])
    face_id = "f:fixture:" + digest(canonical([info["card_id"], info["face_id"]]))[7:23]
    faces.setdefault(face_id, {"id": face_id, "card_id": card_id})
    regions = array(cards[card_id]["regions"])
    if not any(object_value(r)["region"] == region for r in regions):
        regions.append({"region": region, "mapping_state": info["mapping_state"]})
    if kind == "face_revision":
        row |= {"id": owner["id"], "face_id": face_id, "region": region}
        result[kind].append(row)
    else:
        row["face_id"] = face_id
        public_owner["face_id"] = face_id
        result["printing"].append(
            {"id": owner["id"], "card_id": card_id, "region": region, "faces": [row]}
        )
    return public_owner
