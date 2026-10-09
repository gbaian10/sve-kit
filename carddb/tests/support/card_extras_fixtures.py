"""Small synthetic identities and sealed-source metadata, without production text."""

from typing import TYPE_CHECKING

from sve_carddb.build import Json
from sve_carddb.core.json import digest
from sve_carddb.core.provenance import ArchivePin, BuildContext, Source
from sve_carddb.domains.card_extras import CardPage, QAEntry
from sve_carddb.domains.card_extras.archive import PARSER
from sve_carddb.parse.pages import official_en, official_jp

from .build_db_fixtures import rows

if TYPE_CHECKING:
    from sve_carddb.build import Database
    from sve_carddb.core.regions import Region
    from sve_carddb.domains.card_extras import ExtrasPlan

HASH = "sha256:" + "a" * 64
REVISION = "a" * 40


def source(
    number: str = "TEST-001Ⓢa",
    *,
    region: Region = "jp",
    raw: bytes = b"synthetic",
    hour: int = 0,
) -> Source:
    return Source(
        id="src:v1:"
        + digest((region + number + str(hour)).encode()).removeprefix("sha256:"),
        url=(official_jp if region == "jp" else official_en).card_url(number),
        raw_locator="synthetic:raw/example",
        sha256=digest(raw),
        fetched_at=f"2026-10-01T{hour:02d}:00:00Z",
        parser_version=PARSER,
        archive=ArchivePin(
            store_id="synthetic",
            batch_id=HASH,
            descriptor_sha256=HASH,
            first_receipt_id=HASH,
        ),
    )


def page(
    number: str = "TEST-001Ⓢa",
    *,
    region: Region = "jp",
    hour: int = 0,
    question: QAEntry | None = None,
) -> CardPage:
    return CardPage(
        source=source(number, region=region, hour=hour),
        region=region,
        card_no=number,
        qa=(
            question
            or QAEntry(
                stable_source_key="Q900000",
                official_number="Q900000",
                locator="qa-block:0",
                question="Synthetic question?",
                answer="Synthetic answer.",
                published_on="2026-10-01",
            ),
        ),
    )


def context(plan: ExtrasPlan) -> BuildContext:
    return BuildContext.from_inputs(
        REVISION,
        {"card_extras": plan.configuration()},
    )


def seed(db: Database) -> None:
    values = rows()
    with db.transaction():
        for table in (
            "source_record",
            "text_unit",
            "language",
            "product_family",
            "decision",
            "vocabulary",
        ):
            db.insert(table, values[table])
        db.insert(
            "language",
            {
                "code": "en",
                "fallback_order": Json([]),
                "display_name": "Synthetic English",
            },
        )
        for index, (region, number) in enumerate(
            (("jp", "TEST-001Ⓢa"), ("jp", "TEST-002"), ("en", "TEST-001Ⓢa"))
        ):
            card = "card" if index == 0 else f"card{index}"
            face = "face" if index == 0 else f"face{index}"
            printing = "printing" if index == 0 else f"printing{index}"
            db.insert("card", values["card"] | {"id": card})
            db.insert("face", values["face"] | {"id": face, "card_id": card})
            db.insert(
                "printing",
                values["printing"]
                | {
                    "id": printing,
                    "card_id": card,
                    "region": region,
                    "card_no": number,
                },
            )
            db.insert(
                "printing_face",
                values["printing_face"]
                | {"printing_id": printing, "face_id": face, "card_id": card},
            )
