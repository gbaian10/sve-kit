"""Regional exact construction names from current or unanimous observed names."""

from collections import defaultdict
from typing import TYPE_CHECKING

from sve_carddb.core.json import canonical, digest

if TYPE_CHECKING:
    from sve_carddb.build import Database, Value
    from sve_carddb.domains.catalog.models import NameBinding


def _selected_names(db: Database) -> tuple[dict[str, Value], ...]:
    revisions = {row.values["id"]: row.values for row in db.rows("face_revision")}
    selected = {
        (row.values["face_id"], row.values["region"]): dict(row.values)
        for row in db.rows("face_current")
    }
    observed: dict[tuple[Value, Value], dict[Value, Value]] = defaultdict(dict)
    for observation in db.rows("printing_face_observation"):
        revision = revisions[observation.values["revision_id"]]
        observed[revision["face_id"], revision["region"]][revision["name_unit_id"]] = (
            revision["id"]
        )
    for (face, region), names in observed.items():
        if (face, region) not in selected and len(names) == 1:
            selected[face, region] = {
                "face_id": face,
                "region": region,
                "revision_id": next(iter(names.values())),
            }
    return tuple(selected.values())


def populate_rules_names(db: Database) -> int:
    """Group exact regional names without merging cards or counting physical faces."""
    revisions = {row.values["id"]: row.values for row in db.rows("face_revision")}
    texts = {row.values["id"]: row.values for row in db.rows("text_unit")}
    names = {row.values["id"]: row.values for row in db.rows("rules_name")}
    regional = {(row["region"], row["official_name"]): row for row in names.values()}
    links = {
        tuple(row.values[key] for key in ("face_id", "region", "rules_name_id", "role"))
        for row in db.rows("face_rules_name")
    }
    added = 0
    for row in _selected_names(db):
        revision = revisions[row["revision_id"]]
        text = texts[revision["name_unit_id"]]
        region = row["region"]
        assert isinstance(region, str)
        if (revision["face_id"], revision["region"]) != (
            row["face_id"],
            region,
        ) or text["lang"] != ("ja" if region == "jp" else "en"):
            raise ValueError("Construction name face/region/language mismatch")
        name = text["text"]
        if not isinstance(name, str) or not name:
            raise ValueError("Construction name must be nonempty exact text")
        key = region, name
        if key in regional:
            identifier = regional[key]["id"]
        else:
            identifier = "rn:v1:" + digest(canonical([region, name])).removeprefix(
                "sha256:"
            )
            values: dict[str, Value] = {
                "id": identifier,
                "region": region,
                "official_name": name,
                "decision_id": None,
            }
            if identifier in names and names[identifier] != values:
                raise ValueError("Construction name ID collision")
            db.insert("rules_name", values)
            names[identifier] = values
            regional[key] = values
        link = row["face_id"], region, identifier, "primary"
        if any(
            previous[0:2] == link[0:2] and previous[3] == "primary" and previous != link
            for previous in links
        ):
            raise ValueError("Conflicting primary construction name")
        if link not in links:
            db.insert(
                "face_rules_name",
                {
                    "face_id": row["face_id"],
                    "region": region,
                    "rules_name_id": identifier,
                    "role": "primary",
                    "decision_id": None,
                },
            )
            links.add(link)
            added += 1
    return added


def register_name(db: Database, binding: NameBinding) -> None:
    """Add an explicitly confirmed special name only to a face present in that region."""
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
