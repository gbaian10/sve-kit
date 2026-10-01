"""Official product staging and the independently confirmed identity audit trail."""

from typing import TYPE_CHECKING, Protocol

from pydantic import JsonValue

from sve_carddb.build_db import Json
from sve_carddb.build_inputs import SourceUse, input_record, insert_raw_sources
from sve_carddb.registry.inputs import digest
from sve_carddb.snapshot.values import canonical

if TYPE_CHECKING:
    from sve_carddb.build_db import Database, Value
    from sve_carddb.build_inputs import BuildContext, InputRecord
    from sve_carddb.products.identities import ProductIdentities
    from sve_carddb.products.models import LocalizedText
    from sve_carddb.products.plan import OfficialProducts


class TextInterner(Protocol):
    def intern(self, text: LocalizedText) -> str:
        """Intern registered exact text using the build's existing collision checks."""
        ...


def _audit(db: Database, identities: ProductIdentities) -> list[SourceUse]:
    uses: list[SourceUse] = []
    for shard in identities.shards:
        source_id = "authored:v1:" + digest(
            {
                "path": shard.path,
                "hash": shard.checksum,
                "revision": identities.revision,
            }
        ).removeprefix("sha256:")
        db.insert(
            "source_record",
            {
                "id": source_id,
                "kind": "authored",
                "sha256": shard.checksum,
                "authored_path": "authored/" + shard.path,
                "authored_revision": identities.revision,
                "parser_version": "product-identity-v1",
            },
        )
        decision = shard.envelope.decisions[0]
        values = decision.model_dump(
            mode="json", exclude={"members", "reviewed_precision"}
        )
        row: dict[str, Value] = {
            key: value
            for key, value in values.items()
            if isinstance(value, str) or value is None
        }
        row["sample_ids"] = Json(list[JsonValue](decision.sample_ids))
        db.insert("decision", row)
        db.insert(
            "decision_source",
            {
                "decision_id": decision.id,
                "source_id": source_id,
                "role": "product_identity_envelope",
                "locator": shard.path,
            },
        )
        links: set[tuple[str, str]] = set()
        for record in shard.envelope.records:
            for ref in record.evidence:
                checked = identities.evidence[ref]
                role = "product_identity_evidence:" + digest(
                    ref.model_dump(mode="json")
                ).removeprefix("sha256:")
                if (checked.source.id, role) not in links:
                    db.insert(
                        "decision_source",
                        {
                            "decision_id": decision.id,
                            "source_id": checked.source.id,
                            "role": role,
                            "locator": ref.locator,
                        },
                    )
                    links.add((checked.source.id, role))
                uses.append(
                    SourceUse(
                        source=checked.source.model_copy(
                            update={"parser_version": "archive-closure-v1"}
                        ),
                        usage="product_identity_evidence_closure",
                        locator=canonical(ref.model_dump(mode="json")).decode(),
                    )
                )
                if ref.role == "product_identity_match":
                    uses.append(
                        SourceUse(
                            source=checked.source,
                            usage="official_product_identity",
                            locator=ref.locator,
                        )
                    )
    return uses


def populate_official_products(
    db: Database,
    products: OfficialProducts,
    *,
    build: BuildContext,
    texts: TextInterner,
) -> InputRecord:
    """Run in the composing transaction, retaining uses for every unadopted block."""
    products.identities.verify_context(build)
    expected = products.source_uses()
    insert_raw_sources(db, (use.source for use in expected))
    uses = _audit(db, products.identities)
    for page in products.pages:
        uses.append(
            SourceUse(
                source=page.source,
                usage="official_product_page",
                locator=canonical(
                    {"region": page.region, "card_no": page.card_no}
                ).decode(),
            )
        )
        for block in page.blocks:
            uses.append(
                SourceUse(
                    source=page.source, usage="official_product", locator=block.locator
                )
            )
            uses.append(
                SourceUse(
                    source=page.source,
                    usage="official_printing_product",
                    locator=block.locator,
                )
            )
    regions: dict[str, str] = {}
    for product in products.products:
        data = product.data
        regions[data.id] = data.region
        db.insert(
            "product",
            {
                "id": data.id,
                "region": data.region,
                "family_id": data.family_id,
                "product_code": data.product_code,
                "name_unit_id": texts.intern(data.name),
                "product_type": data.product_type,
                "released_on": data.released_on,
                "date_precision": data.date_precision,
                "date_raw": data.date_raw,
                "source_id": product.page.source.id,
            },
        )
    printings = {row.values["id"]: row.values for row in db.rows("printing")}
    for inclusion in products.inclusions:
        inclusion_data = inclusion.data
        printing = printings.get(inclusion_data.printing_id)
        if printing is None or printing["region"] != regions.get(
            inclusion_data.product_id
        ):
            raise ValueError(
                "Official inclusion requires an adopted same-region printing/product"
            )
        if printing["source_id"] != inclusion.page.source.id:
            raise ValueError(
                "Official inclusion source differs from adopted printing source"
            )
        db.insert(
            "printing_product",
            {
                "printing_id": inclusion_data.printing_id,
                "product_id": inclusion_data.product_id,
                "first_available_on": inclusion_data.first_available_on,
                "first_available_precision": inclusion_data.first_available_precision,
                "first_available_raw": inclusion_data.first_available_raw,
                "inclusion_kind": inclusion_data.inclusion_kind,
                "note_unit_id": None
                if inclusion_data.note is None
                else texts.intern(inclusion_data.note),
                "source_id": inclusion.page.source.id,
            },
        )
    inputs = input_record(build, uses)
    inputs.verify(db, build, expected, complete=False)
    return inputs
