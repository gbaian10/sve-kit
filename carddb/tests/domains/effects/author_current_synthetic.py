"""Explicit synthetic mutations around the public production database fixture."""

from copy import deepcopy
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.core.json import array, digest, object_value

from .author_current_goldens import FACTS

if TYPE_CHECKING:
    import pytest

    from sve_carddb.core.provenance import Source
    from sve_carddb.domains.catalog.adoption_models import SourceRef
    from sve_carddb.domains.effects.author_current import CurrentAuthorSources


def effect(
    current: CurrentAuthorSources, monkeypatch: pytest.MonkeyPatch, text: str
) -> None:
    revision = FACTS["BP01-009"]["revision"]
    row = current.db.select(
        "face_revision", current.db.columns("face_revision"), where={"id": revision}
    )[0]
    identifier = "unit:synthetic-effect"
    with current.db.transaction():
        existing = current.db.select(
            "text_unit",
            ("id",),
            where={"lang": "ja", "content_hash": digest(text.encode())},
        )
        if existing:
            value = existing[0].values["id"]
            assert isinstance(value, str)
            identifier = value
        else:
            current.db.insert(
                "text_unit",
                {
                    "id": identifier,
                    "lang": "ja",
                    "text": text,
                    "content_hash": digest(text.encode()),
                },
            )
        current.db.update(
            "face_revision", {"id": revision}, {"effect_unit_id": identifier}
        )
    original = current.sources.document

    def document(ref: SourceRef) -> tuple[str, JsonValue, Source]:
        lang, value, source = original(ref)
        if source.id == row.values["source_id"]:
            value = deepcopy(value)
            object_value(array(object_value(value)["faces"])[0])["text"] = text
        return lang, value, source

    monkeypatch.setattr(current.sources, "document", document)


def back_face(
    current: CurrentAuthorSources, monkeypatch: pytest.MonkeyPatch
) -> tuple[str, str]:
    db = current.db
    face_id, revision_id = "f:" + "e" * 32, "revision:synthetic-back"
    card_id = "c:9c5459446e0254818099800b4208cd96"
    old_revision = FACTS["BP01-034"]["revision"]
    revision = dict(
        db.select(
            "face_revision", db.columns("face_revision"), where={"id": old_revision}
        )[0].values
    )
    source_id = revision["source_id"]
    revision.update(id=revision_id, face_id=face_id)
    with db.transaction():
        db.update("card", {"id": card_id}, {"layout": "double_faced"})
        db.insert(
            "face", {"id": face_id, "card_id": card_id, "ordinal": 1, "side": "back"}
        )
        db.insert("face_revision", revision)
        db.insert(
            "face_current",
            {
                "face_id": face_id,
                "region": "jp",
                "revision_id": revision_id,
                "basis": "latest_observed_no_errata",
            },
        )
        for table in ("face_trait", "face_title", "face_special_kind"):
            for row in db.select(
                table, db.columns(table), where={"revision_id": old_revision}
            ):
                db.insert(table, dict(row.values) | {"revision_id": revision_id})
        for row in db.select(
            "face_rules_name",
            db.columns("face_rules_name"),
            where={"face_id": FACTS["BP01-034"]["face"], "region": "jp"},
        ):
            db.insert("face_rules_name", dict(row.values) | {"face_id": face_id})
    original = current.sources.document

    def document(ref: SourceRef) -> tuple[str, JsonValue, Source]:
        lang, value, source = original(ref)
        if source.id == source_id:
            value = deepcopy(value)
            faces = array(object_value(value)["faces"])
            faces.append(deepcopy(faces[0]))
        return lang, value, source

    monkeypatch.setattr(current.sources, "document", document)
    return face_id, revision_id


def section(current: CurrentAuthorSources, monkeypatch: pytest.MonkeyPatch) -> None:
    db = current.db
    revision = FACTS["BP01-009"]["revision"]
    row = db.select(
        "face_revision", db.columns("face_revision"), where={"id": revision}
    )[0]
    effect_unit = row.values["effect_unit_id"]
    unit = db.select("text_unit", ("text",), where={"id": effect_unit})[0]
    text = unit.values["text"]
    assert isinstance(text, str)
    with db.transaction():
        db.insert(
            "face_text_section",
            {
                "revision_id": revision,
                "ordinal": 0,
                "text_unit_id": effect_unit,
                "kind": "rule",
            },
        )
    original = current.sources.document

    def document(ref: SourceRef) -> tuple[str, JsonValue, Source]:
        lang, value, source = original(ref)
        if source.id == row.values["source_id"]:
            value = deepcopy(value)
            object_value(array(object_value(value)["faces"])[0])["sections"] = [text]
        return lang, value, source

    monkeypatch.setattr(current.sources, "document", document)
