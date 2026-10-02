"""Preserve complete canonical rules and map only explicitly checked revisions."""

from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build_db import Json
from sve_carddb.products.models import LocalizedText
from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.text_observations.models import candidate_revision_id

if TYPE_CHECKING:
    from sve_carddb.build_db import Database, Value
    from sve_carddb.text_observations.intern import TextInterner
    from sve_carddb.text_observations.models import FaceContent
    from sve_carddb.wording_adoptions.replay import ReplayedAdoption


def rule_hash(content: FaceContent) -> str:
    """Do not discard any unknown section, condition, reminder or token text."""
    if content.effect is None:
        raise ValueError("Missing rule text cannot create semantics")
    return digest(
        canonical(
            {"rule_text": content.effect, "rule_sections": list(content.sections)}
        )
    )


def populate_semantics(
    db: Database, adoption: ReplayedAdoption, texts: TextInterner
) -> str:
    """Reuse a predecessor's immutable canonical representation after full replay."""
    if not all(
        db.has_table(name)
        for name in ("face_semantics", "revision_semantics", "semantic_reference")
    ):
        raise ValueError("Confirmed wording adoption requires the semantics capability")
    data = adoption.record.data
    mappings = {
        r.values["revision_id"]: r.values["semantic_id"]
        for r in db.rows("revision_semantics")
    }
    canonical_item = adoption.previous or adoption.selected
    previous_id = (
        None if adoption.previous is None else candidate_revision_id(adoption.previous)
    )
    semantic_id = mappings.get(previous_id)
    if semantic_id is None:
        checksum = rule_hash(canonical_item.content)
        normalizer = "wording-preserve-all-v1"
        semantic_id = "sem:v1:" + digest(
            canonical([data.face_id, data.region, normalizer, checksum])
        ).removeprefix("sha256:")
        content = canonical_item.content
        assert content.effect is not None
        lang = "ja" if data.region == "jp" else "en"
        values: dict[str, Value] = {
            "id": semantic_id,
            "face_id": data.face_id,
            "region": data.region,
            "rule_text_unit_id": texts.intern(
                LocalizedText(lang=lang, text=content.effect)
            ),
            "rule_sections": Json(
                [
                    texts.intern(LocalizedText(lang=lang, text=s))
                    for s in content.sections
                ]
            ),
            "normalizer_version": normalizer,
            "rule_hash": checksum,
            "decision_id": adoption.decision.id,
        }
        existing = next(
            (
                r.values
                for r in db.rows("face_semantics")
                if r.values["id"] == semantic_id
            ),
            None,
        )
        if existing is None:
            db.insert("face_semantics", values)
        elif {k: v for k, v in existing.items() if k != "decision_id"} != {
            k: v for k, v in values.items() if k != "decision_id"
        }:
            raise ValueError("Immutable semantic representation conflicts")
    if not isinstance(semantic_id, str):
        raise TypeError("Invalid semantic identity at the database boundary")
    items = list(adoption.scope.contents.values())
    if adoption.previous is not None:
        items.append(adoption.previous)
    for item in items:
        revision = candidate_revision_id(item)
        if revision in mappings:
            if mappings[revision] != semantic_id:
                raise ValueError("Checked revision already has incompatible semantics")
        else:
            db.insert(
                "revision_semantics",
                {
                    "revision_id": revision,
                    "semantic_id": semantic_id,
                    "decision_id": adoption.decision.id,
                },
            )
            mappings[revision] = semantic_id
    return semantic_id


def verify_semantics(db: Database) -> None:
    """Validate the exact canonical rule hash independently of the importer."""
    units = {r.values["id"]: r.values["text"] for r in db.rows("text_unit")}
    for row in db.rows("face_semantics"):
        values = row.values
        sections = values["rule_sections"]
        if not isinstance(sections, Json) or not isinstance(sections.value, list):
            raise TypeError("Invalid semantic sections")
        text = units.get(values["rule_text_unit_id"])
        strings: list[JsonValue] = []
        for key in sections.value:
            if not isinstance(key, str) or not isinstance(units.get(key), str):
                raise TypeError("Semantic section text dependency is missing")
            value = units[key]
            if not isinstance(value, str):
                raise TypeError("Semantic section requires exact text")
            strings.append(value)
        if (
            not isinstance(text, str)
            or digest(canonical({"rule_text": text, "rule_sections": strings}))
            != values["rule_hash"]
        ):
            raise ValueError(
                "Immutable semantic rule hash does not match exact content"
            )


def bundle_ready(db: Database, revision: str) -> bool:
    """Unknown section kinds prevent verified DSL reuse even after wording adoption."""
    if not any(
        r.values["revision_id"] == revision for r in db.rows("revision_semantics")
    ):
        return False
    mapping = next(
        r.values
        for r in db.rows("revision_semantics")
        if r.values["revision_id"] == revision
    )
    checked = {
        r.values["revision_id"]
        for r in db.rows("revision_semantics")
        if r.values["semantic_id"] == mapping["semantic_id"]
    }
    if any(
        r.values["revision_id"] in checked and r.values["kind"] == "unknown"
        for r in db.rows("face_text_section")
    ):
        return False
    # Token dependency normalization is not implemented; retain it and refuse reuse.
    return not any(
        r.values["semantic_id"] == mapping["semantic_id"]
        for r in db.rows("semantic_reference")
    )
