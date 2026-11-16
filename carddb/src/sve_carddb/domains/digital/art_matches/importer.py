"""Materialize current art assertions only against this build's fresh same-card scope."""

from typing import TYPE_CHECKING

from sve_carddb.build.rows import insert_exact
from sve_carddb.core.json import array, canonical, digest, integer, object_value, parse
from sve_carddb.domains.digital.links.evidence import configured_refs
from sve_carddb.domains.translations.digital import import_digital
from sve_carddb.images.art_matching import REVIEW_MAX

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.build import Database, Value
    from sve_carddb.domains.digital.art_matches.loader import Snapshot
    from sve_carddb.domains.digital.art_matches.models import Record
    from sve_carddb.domains.digital.links.importer import Result as LinkResult
    from sve_carddb.domains.translations.sources import Sources


def _catalogue(sources: Sources) -> dict[tuple[str, str], dict[str, JsonValue]]:
    result = {}
    for ref in configured_refs(sources):
        lang, document, _source = sources.document(ref)
        if lang != "ja":
            continue
        game = ref.parser.removeprefix("translation-").removesuffix("-v1")
        data = object_value(object_value(document)["data"])
        entries = (
            array(data["cards"])
            if game == "sv1"
            else object_value(data["card_details"]).values()
        )
        for raw in entries:
            item = object_value(raw)
            common = item if game == "sv1" else object_value(item["common"])
            result[game, str(integer(common["card_id"]))] = item
    return result


def _images(game: str, item: dict[str, JsonValue]) -> set[tuple[str, str]]:
    if game == "sv1":
        resource = str(integer(item["resource_card_id"]) or integer(item["card_id"]))
        return {
            (phase, resource)
            for phase in (
                ("normal", "evolved") if item["char_type"] == 1 else ("normal",)
            )
        }
    common = object_value(item["common"])
    hashes = {("normal", str(common["card_image_hash"]))}
    if item["evo"]:
        hashes.add(("evolved", str(object_value(item["evo"])["card_image_hash"])))
    for raw in array(item["style_card_list"]):
        style = object_value(raw)
        hashes.add(("normal", str(style["hash"])))
        if style["evo_hash"]:
            hashes.add(("evolved", str(style["evo_hash"])))
    return hashes


def _current_images(
    record: Record, catalogue: dict[tuple[str, str], dict[str, JsonValue]]
) -> bool:
    required = set()
    for official in record.same_card_targets:
        item = catalogue.get((record.game, official))
        if item is None:
            return False
        required.update(_images(record.game, item))
    present = set()
    for candidate in record.candidates:
        item = catalogue.get((record.game, candidate.official_id))
        if item is None:
            return False
        image = (
            candidate.official_id
            if record.game == "sv1"
            else candidate.image_url.rsplit("/", 1)[-1].removesuffix(".png")
        )
        key = candidate.phase, image
        if key not in _images(record.game, item):
            return False
        present.add(key)
    return record.state != "not_found" or required <= present


def _source(db: Database, path: str, sha256: str, revision: str) -> str:
    identifier = (
        "authored:digital-art:" + digest(canonical([path, sha256, revision]))[7:]
    )
    insert_exact(
        db,
        "source_record",
        {
            "id": identifier,
            "kind": "authored",
            "url": None,
            "raw_locator": None,
            "etag": None,
            "last_modified": None,
            "fetched_at": None,
            "sha256": sha256,
            "authored_path": "authored/" + path,
            "authored_revision": revision,
            "parser_version": "digital-art-authored-v1",
        },
        ("id",),
    )
    return identifier


def _audit(db: Database, record: Record, source: str, path: str) -> str:
    decision = (
        "d:" + digest(canonical(["digital-art-v1", record.model_dump(mode="json")]))[7:]
    )
    insert_exact(
        db,
        "decision",
        {
            "id": decision,
            "state": record.review_level,
            "scope": "record",
            "category": "digital_art",
            "membership_hash": None,
            "policy_id": None,
            "sample_ids": None,
            "confidence": None,
            "note": record.reason,
        },
        ("id",),
    )
    insert_exact(
        db,
        "decision_source",
        {
            "decision_id": decision,
            "source_id": source,
            "role": "digital_art_shard",
            "locator": path,
            "quote": None,
        },
        ("decision_id", "source_id", "role"),
    )
    return decision


def _fresh(
    db: Database,
    snapshot: Snapshot,
    *,
    links: LinkResult | None,
    sources: Sources,
) -> list[tuple[Record, str]]:
    art = {str(r.values["id"]): r.values for r in db.rows("art")}
    catalogue = _catalogue(sources)
    uses = {
        (r.values["printing_id"], r.values["face_id"]): r.values["art_id"]
        for r in db.rows("printing_face")
    }
    targets: dict[tuple[str, str | None, str], set[str]] = {}
    if links is not None:
        for (card, face), records in links.by_owner.items():
            for link in records:
                if link.value.relation == "same_card":
                    targets.setdefault((card, face, link.subject.game), set()).add(
                        link.subject.official_id
                    )
    fresh: list[tuple[Record, str]] = []
    for record in snapshot.records():
        parent = art.get(record.art_id)
        if parent is None:
            continue
        if (parent["card_id"], parent["face_id"]) != (record.card_id, record.face_id):
            raise ValueError("Art comparison belongs to another registered face")
        current = targets.get((record.card_id, None, record.game), set()) | targets.get(
            (record.card_id, record.face_id, record.game), set()
        )
        witness = record.witness
        valid = (
            set(record.same_card_targets) == current
            and _current_images(record, catalogue)
            and (
                witness is not None
                and witness.face_id == record.face_id
                and uses.get((witness.printing_id, witness.face_id)) == record.art_id
            )
        )
        fresh.append((record, record.state if valid else "unknown"))
    return fresh


def _matches(db: Database, record: Record, decision: str, source: str) -> None:
    for candidate in record.candidates:
        if candidate.inliers < REVIEW_MAX:
            continue
        card = f"digital:{record.game}:{candidate.official_id}"
        identifier = f"digital-art:{record.game}:{candidate.official_id}:{candidate.phase}:{candidate.style_key}"
        values: dict[str, Value] = {
            "id": identifier,
            "digital_face_id": card + ":" + candidate.phase,
            "style_key": candidate.style_key,
            "source_card_id": card,
            "source_image_url": None,
            "image_hash_raw": None,
            "illustrator_raw": None,
            "source_id": source,
        }
        insert_exact(db, "digital_art", values, ("id",))
        insert_exact(
            db,
            "digital_art_link",
            {
                "art_id": record.art_id,
                "digital_art_id": identifier,
                "relation": "same_art",
                "decision_id": decision,
            },
            ("art_id", "digital_art_id"),
        )


def populate(
    db: Database,
    snapshot: Snapshot,
    *,
    revision: str,
    links: LinkResult | None,
    sources: Sources,
) -> dict[str, int]:
    """Run in the caller's transaction; missing or changed scope remains unknown."""
    config = object_value(parse(sources.build.configuration.encode()))
    if (
        config.get("digital_art_authored")
        != snapshot.configuration()["digital_art_authored"]
    ):
        raise ValueError("Build configuration does not pin digital art inputs")
    fresh = _fresh(db, snapshot, links=links, sources=sources)
    positive = tuple(
        sorted(
            {
                (record.game, candidate.official_id)
                for record, state in fresh
                if state == "matched"
                for candidate in record.candidates
                if candidate.inliers >= REVIEW_MAX
            }
        )
    )
    if positive:
        import_digital(
            db, sources, configured_refs(sources), positive, allow_subset=True
        )
    index_source = _source(
        db, "digital/art-matches/index.yaml", snapshot.index_hash, revision
    )
    locations = {}
    for path, raw, shard in snapshot.shards:
        source = _source(db, path, digest(raw), revision)
        for record in shard.records:
            locations[record.art_id, record.game] = source, path
    counts: dict[str, int] = {"matched": 0, "not_found": 0, "unknown": 0}
    for record, state in fresh:
        source, path = locations[record.art_id, record.game]
        decision = _audit(db, record, source, path)
        if state == "matched":
            _matches(db, record, decision, index_source)
        insert_exact(
            db,
            "digital_art_coverage",
            {
                "art_id": record.art_id,
                "game": record.game,
                "state": state,
                "as_of": snapshot.index.as_of,
                "decision_id": decision,
            },
            ("art_id", "game"),
        )
        counts[state] += 1
    return counts
