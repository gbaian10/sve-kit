"""Regional exact construction names from current or unanimous observed names."""

from collections import defaultdict
from typing import TYPE_CHECKING

from sve_carddb.core.json import canonical, digest

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.build import Database, Value
    from sve_carddb.domains.catalog.models import NameBinding


class _Names:
    def __init__(self, db: Database) -> None:
        self.revisions = {
            row.values["id"]: row.values for row in db.rows("face_revision")
        }
        self.texts = {row.values["id"]: row.values for row in db.rows("text_unit")}

    def name(self, revision_id: Value) -> tuple[Value, Value, Value]:
        revision = self.revisions[revision_id]
        return (
            revision["face_id"],
            revision["region"],
            self.texts[revision["name_unit_id"]]["text"],
        )

    def checked(self, revision_id: Value) -> str:
        revision = self.revisions[revision_id]
        text = self.texts[revision["name_unit_id"]]
        value = text["text"]
        if text["lang"] != ("ja" if revision["region"] == "jp" else "en"):
            raise ValueError("Construction name face/region/language mismatch")
        if not isinstance(value, str) or not value:
            raise ValueError("Construction name must be nonempty exact text")
        return value


def _selected_names(
    db: Database, collab: dict[tuple[Value, Value], set[str]]
) -> dict[tuple[Value, Value], str]:
    """Current, else the unanimous observed name, both after declared collab names.

    A current revision showing a collab name stands aside so the remaining printings
    decide; collab names never become the primary construction name.
    """
    lookup = _Names(db)
    name, checked = lookup.name, lookup.checked
    selected: dict[tuple[Value, Value], str] = {}
    for row in db.rows("face_current"):
        face, region, value = name(row.values["revision_id"])
        if (face, region) != (row.values["face_id"], row.values["region"]):
            raise ValueError("Construction name face/region/language mismatch")
        if value not in collab.get((face, region), set()):
            selected[face, region] = checked(row.values["revision_id"])
    observed: dict[tuple[Value, Value], dict[Value, Value]] = defaultdict(dict)
    for observation in db.rows("printing_face_observation"):
        face, region, value = name(observation.values["revision_id"])
        if value not in collab.get((face, region), set()):
            observed[face, region][value] = observation.values["revision_id"]
    for key, revisions in observed.items():
        if key not in selected and len(revisions) == 1:
            selected[key] = checked(next(iter(revisions.values())))
    return selected


def _rules_name(
    db: Database,
    names: dict[Value, Mapping[str, Value]],
    regional: dict[tuple[Value, Value], Mapping[str, Value]],
    region: Value,
    official_name: str,
) -> Value:
    if not isinstance(region, str):
        raise TypeError("Construction name region must be text")
    key = region, official_name
    if key in regional:
        return regional[key]["id"]
    identifier = "rn:v1:" + digest(canonical([region, official_name])).removeprefix(
        "sha256:"
    )
    values: dict[str, Value] = {
        "id": identifier,
        "region": region,
        "official_name": official_name,
        "decision_id": None,
    }
    if identifier in names and names[identifier] != values:
        raise ValueError("Construction name ID collision")
    db.insert("rules_name", values)
    names[identifier] = values
    regional[key] = values
    return identifier


def _split(
    adopted: tuple[NameBinding, ...],
) -> tuple[dict[tuple[Value, Value], set[str]], dict[tuple[Value, Value], str]]:
    collab: dict[tuple[Value, Value], set[str]] = defaultdict(set)
    explicit: dict[tuple[Value, Value], str] = {}
    for binding in adopted:
        key = binding.face_id, binding.region
        if binding.role == "collab":
            collab[key].add(binding.official_name)
        elif binding.role == "primary":
            if key in explicit:
                raise ValueError("A face region has one explicit primary name")
            explicit[key] = binding.official_name
    return collab, explicit


def populate_rules_names(db: Database, adopted: tuple[NameBinding, ...] = ()) -> int:
    """Group exact regional names without merging cards or counting physical faces.

    ``adopted`` holds checked, still-current special names: collab names are kept
    out of primary derivation, and an explicit primary is used only where no
    printing derives one.
    """
    names = {row.values["id"]: row.values for row in db.rows("rules_name")}
    regional = {(row["region"], row["official_name"]): row for row in names.values()}
    primary: dict[tuple[Value, Value], set[Value]] = defaultdict(set)
    for existing in db.rows("face_rules_name"):
        if existing.values["role"] == "primary":
            primary[existing.values["face_id"], existing.values["region"]].add(
                existing.values["rules_name_id"]
            )
    collab, explicit = _split(adopted)
    selected = _selected_names(db, collab)
    if explicit.keys() & selected.keys():
        raise ValueError("Explicit primary name where observations derive one")
    added = 0
    for (face, region), official_name in sorted({**selected, **explicit}.items()):
        identifier = _rules_name(db, names, regional, region, official_name)
        previous = primary[face, region]
        if previous and previous != {identifier}:
            raise ValueError("Conflicting primary construction name")
        if identifier not in previous:
            db.insert(
                "face_rules_name",
                {
                    "face_id": face,
                    "region": region,
                    "rules_name_id": identifier,
                    "role": "primary",
                    "decision_id": None,
                },
            )
            previous.add(identifier)
            added += 1
    for binding in adopted:
        if binding.role != "primary":
            register_name(db, binding)
    return added


def register_name(db: Database, binding: NameBinding) -> None:
    """Add an explicitly confirmed special name only to a face present in that region."""
    if binding.role == "primary":
        raise ValueError("Primary construction names are derived, not registered")
    printings = {row.values["id"]: row.values["region"] for row in db.rows("printing")}
    if not any(
        row.values["face_id"] == binding.face_id
        and printings[row.values["printing_id"]] == binding.region
        for row in db.rows("printing_face")
    ):
        raise ValueError("Special construction name has no face in its region")
    names = [
        row.values
        for row in db.rows("rules_name")
        if (row.values["region"], row.values["official_name"])
        == (binding.region, binding.official_name)
    ]
    if names:
        identifier = names[0]["id"]
    else:
        identifier = "rn:v1:" + digest(
            canonical([binding.region, binding.official_name])
        ).removeprefix("sha256:")
        db.insert(
            "rules_name",
            {
                "id": identifier,
                "region": binding.region,
                "official_name": binding.official_name,
                "decision_id": binding.decision_id,
            },
        )
    values: dict[str, Value] = {
        "face_id": binding.face_id,
        "region": binding.region,
        "rules_name_id": identifier,
        "role": binding.role,
        "decision_id": binding.decision_id,
    }
    existing = [
        row.values
        for row in db.rows("face_rules_name")
        if all(
            row.values[key] == values[key]
            for key in ("face_id", "region", "rules_name_id", "role")
        )
    ]
    if existing and existing != [values]:
        raise ValueError("Conflicting special construction name decision")
    if not existing:
        db.insert("face_rules_name", values)
