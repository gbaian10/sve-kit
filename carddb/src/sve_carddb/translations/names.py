"""Materialize a single adopted digital name, without rendering or selecting effects."""

from typing import TYPE_CHECKING

from sve_carddb.build_inputs import SourceUse, insert_raw_sources
from sve_carddb.catalog.adoption_models import SourceRef
from sve_carddb.snapshot.values import array, canonical, digest, object_value, parse
from sve_carddb.translations.digital import _phases, select_name
from sve_carddb.translations.name_build import NameOwner, name_context, name_source
from sve_carddb.translations.name_materialization import materialize_name
from sve_carddb.translations.name_selection import NameCandidate

if TYPE_CHECKING:
    from sve_carddb.build_db import Database
    from sve_carddb.build_inputs import Source
    from sve_carddb.digital_links.importer import Result
    from sve_carddb.translations.sources import Sources


def _proof(  # ruff: ignore[too-many-locals] -- exact names need both frozen API identity and language pins
    sources: Sources, game: str, official: str, phase: str, lang: str, text: str
) -> tuple[SourceRef, Source]:
    config = object_value(parse(sources.build.configuration.encode()))
    digital = object_value(config.get("digital_evidence"))
    found = []
    for value in array(digital.get("refs")):
        ref = SourceRef.model_validate_json(canonical(value))
        if ref.parser != "translation-" + game + "-v1":
            continue
        actual_lang, document, source = sources.document(ref)
        if actual_lang != lang:
            continue
        data = object_value(object_value(document).get("data"))
        if game == "svwb":
            details = object_value(data.get("card_details"))
            if official not in details:
                continue
            item = object_value(details[official])
            common = object_value(item.get("common"))
            locator = f"/data/card_details/{official}/common/name"
            name = common.get("name")
        else:
            matches = [
                (i, object_value(c))
                for i, c in enumerate(array(data.get("cards")))
                if str(object_value(c).get("card_id")) == official
            ]
            if len(matches) != 1:
                continue
            index, common = matches[0]
            item = common
            locator = f"/data/cards/{index}/card_name"
            name = common.get("card_name")
        if phase not in _phases(game, common, item):
            raise ValueError("Selected digital face is absent from frozen source")
        if name != text:
            raise ValueError("Selected digital name differs from frozen source")
        exact = ref.model_copy(
            update={"locator": locator, "text_hash": digest(text.encode())}
        )
        sources.text(exact)
        found.append((exact, source))
    if not found:
        raise ValueError("Selected digital name lacks frozen language evidence")
    return min(found, key=lambda item: canonical(item[0].model_dump(mode="json")))


def populate_name_translation(  # ruff: ignore[too-many-locals,complex-structure] -- owner, raw proof and stable output are composed in one transaction
    db: Database,
    sources: Sources,
    *,
    revision_id: str,
    lang: str,
    links: Result | None = None,
) -> str | None:
    """Use the caller's publication DB, including pending wording; no current filter."""
    config = object_value(parse(sources.build.configuration.encode()))
    if "digital_link_authored" in config and links is None:
        raise ValueError("Authored digital names require owner evidence result")
    revisions = {r.values["id"]: r.values for r in db.rows("face_revision")}
    revision = revisions[revision_id]
    if revision["region"] != "jp" or lang != "zh-Hant":
        raise ValueError(
            "Name evidence import requires JP to Traditional Chinese; regional selection belongs to #53"
        )
    faces = {r.values["id"]: r.values for r in db.rows("face")}
    face = faces[revision["face_id"]]
    eligible = None if links is None else links.eligible(db, sources, revision_id)
    chosen = select_name(
        db,
        card_id=str(face["card_id"]),
        face_id=str(face["id"]),
        lang=lang,
        eligible_links=eligible,
    )
    if chosen is None:
        return None
    text, origin, decision_id = chosen
    units = {r.values["id"]: r.values for r in db.rows("text_unit")}
    unit = units[revision["name_unit_id"]]
    if unit["lang"] != "ja":
        raise ValueError("JP name source language mismatch")
    if not isinstance(unit["text"], str) or unit["content_hash"] != digest(
        unit["text"].encode()
    ):
        raise ValueError("Source name exact hash mismatch")
    for other in revisions.values():
        if other["name_unit_id"] != unit["id"]:
            continue
        owner = faces[other["face_id"]]
        alternative = select_name(
            db,
            card_id=str(owner["card_id"]),
            face_id=str(owner["id"]),
            lang=lang,
            eligible_links=None
            if links is None
            else links.eligible(db, sources, str(other["id"])),
        )
        if alternative is not None and alternative[0] != text:
            raise ValueError(
                "Ambiguous source name requires adopted context assignment"
            )
    cards = {r.values["id"]: r.values for r in db.rows("digital_card")}
    matching_faces = {
        r.values["digital_face_id"]
        for r in db.rows("digital_text")
        if r.values["lang"] == lang and units[r.values["name_unit_id"]]["text"] == text
    }
    matching_links = [
        r.values
        for r in db.rows("digital_link")
        if (eligible is None or r.values["id"] in eligible)
        and r.values["decision_id"] == decision_id
        and r.values["face_id"] == face["id"]
        and r.values["card_id"] == face["card_id"]
        and r.values["relation"] == "same_card"
        and r.values["digital_face_id"] in matching_faces
        and cards[r.values["digital_card_id"]]["game"]
        == origin.removeprefix("official_")
    ]
    if not matching_links:
        raise ValueError("Selected digital name lacks an eligible owner link")
    link = min(matching_links, key=lambda row: str(row["id"]))
    card = cards[link["digital_card_id"]]
    digital_faces = {r.values["id"]: r.values for r in db.rows("digital_face")}
    phase = str(digital_faces[link["digital_face_id"]]["phase"])
    ref, source = _proof(
        sources, str(card["game"]), str(card["official_id"]), phase, lang, text
    )
    insert_raw_sources(db, (source,))
    sources.uses.append(
        SourceUse(source=source, usage="digital_name_translation", locator=ref.locator)
    )
    owner_source = name_source(db, NameOwner("face_revision", revision_id))
    if owner_source is None:
        return None
    context = name_context(db, owner_source)
    decision = next(
        r.values for r in db.rows("decision") if r.values["id"] == decision_id
    )
    if decision["reviewed_at"] is None:
        raise ValueError("Adopted name decision lacks its actual review date")
    return materialize_name(
        db,
        owner_source,
        context,
        NameCandidate(
            text,
            origin,
            "digital_official",
            decision_id,
            str(decision["reviewed_at"]),
            source,
            (ref,),
        ),
    )
