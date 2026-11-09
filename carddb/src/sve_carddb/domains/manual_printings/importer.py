"""Stage manual unlisted printings into the typed build database."""

import re
from typing import TYPE_CHECKING

from sve_carddb.core.json import canonical, digest
from sve_carddb.core.provenance import BuildContext, InputRecord, input_record
from sve_carddb.domains.manual_printings.records import PARSER_VERSION
from sve_carddb.domains.products.models import LocalizedText
from sve_carddb.domains.text_observations.intern import TextInterner

if TYPE_CHECKING:
    from sve_carddb.build.database import Database
    from sve_carddb.domains.manual_printings.loader import ManualSnapshot


def populate_manual_printings(
    db: Database,
    snapshot: ManualSnapshot,
    *,
    authored_revision: str,
    build: BuildContext,
) -> InputRecord:
    """Insert provisional cards, faces and unlisted printings inside the caller's transaction.

    The manual name is a separate text unit on the printing; no face revision, current
    or rules name is derived from it, so the face stays textless like an unknown source.
    """
    if not re.fullmatch(r"[0-9a-f]{40}", authored_revision):
        raise ValueError("Authored revision must be a full Git commit SHA")
    families = {row.values["id"] for row in db.rows("product_family")}
    texts = TextInterner(db)
    sources: dict[str, str] = {}
    for shard in snapshot.shards:
        source_id = "authored:v1:" + digest(
            canonical(
                {
                    "path": shard.path,
                    "hash": shard.content_hash,
                    "revision": authored_revision,
                }
            )
        ).removeprefix("sha256:")
        db.insert(
            "source_record",
            {
                "id": source_id,
                "kind": "authored",
                "sha256": shard.content_hash,
                "authored_path": "authored/" + shard.path,
                "authored_revision": authored_revision,
                "parser_version": PARSER_VERSION,
            },
        )
        for entry in shard.records:
            sources[entry.record_key] = source_id
    for entry in sorted(snapshot.records.values(), key=lambda item: item.record_key):
        data = entry.data
        if data.home_set_id not in families:
            raise ValueError(
                "Missing product_family parent for manual printing; supply the family first"
            )
        source_id = sources[entry.record_key]
        db.insert(
            "card",
            {
                "id": data.card_id,
                "layout": "single",
                "identity_state": "provisional",
                "home_set_id": data.home_set_id,
            },
        )
        db.insert(
            "face",
            {
                "id": data.face_id,
                "card_id": data.card_id,
                "ordinal": 0,
                "side": "front",
            },
        )
        db.insert(
            "printing",
            {
                "id": data.id,
                "card_id": data.card_id,
                "region": data.region,
                "card_no": "" if data.card_no is None else data.card_no,
                "card_no_state": data.card_no_state,
                "catalog_state": "unlisted",
                "decklog_available": False,
                "decklog_verification": "unverified",
                "listing_confidence": "high",
                "variant_key": "standard",
                "home_set_id": data.home_set_id,
                # The card face prints no rarity; "-" is the official label for none.
                "rarity_raw": "-",
                "printed_code": data.printed_code,
                "manual_type_code": data.card_type,
                "manual_name_unit_id": texts.intern(
                    LocalizedText(lang=data.name.lang, text=data.name.text)
                ),
                "source_id": source_id,
            },
        )
        db.insert(
            "printing_face",
            {
                "printing_id": data.id,
                "face_id": data.face_id,
                "card_id": data.card_id,
                "embellishment_state": "unreviewed",
                "printed_text_state": "unknown",
                "source_id": source_id,
            },
        )
        db.insert("card_int_id", {"int_id": data.int_id, "printing_id": data.id})
        for url in data.references:
            db.insert(
                "printing_reference",
                {"printing_id": data.id, "url": url, "source_id": source_id},
            )
    return input_record(build, ())
