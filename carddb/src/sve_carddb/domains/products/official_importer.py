"""Official product staging keyed by independently confirmed product identities."""

from typing import TYPE_CHECKING, Protocol

from sve_carddb.build.source_rows import insert_raw_sources
from sve_carddb.core.json import canonical, digest
from sve_carddb.core.provenance import SourceUse, input_record

if TYPE_CHECKING:
    from sve_carddb.build import Database
    from sve_carddb.core.provenance import BuildContext, InputRecord
    from sve_carddb.domains.products.identities import ProductIdentities
    from sve_carddb.domains.products.models import LocalizedText
    from sve_carddb.domains.products.plan import OfficialProducts


class TextInterner(Protocol):
    def intern(self, text: LocalizedText) -> str:
        """Intern registered exact text using the build's existing collision checks."""
        ...


def _authored_sources(db: Database, identities: ProductIdentities) -> None:
    for shard in identities.shards:
        db.insert(
            "source_record",
            {
                "id": "authored:v1:"
                + digest(
                    canonical(
                        {
                            "path": shard.path,
                            "hash": shard.checksum,
                            "revision": identities.revision,
                        }
                    )
                ).removeprefix("sha256:"),
                "kind": "authored",
                "sha256": shard.checksum,
                "authored_path": "authored/" + shard.path,
                "authored_revision": identities.revision,
                "parser_version": "product-identity-v3",
            },
        )


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
    _authored_sources(db, products.identities)
    uses = list(products.identities.source_uses())
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
            uses.extend(
                SourceUse(source=page.source, usage=usage, locator=block.locator)
                for usage in ("official_product", "official_printing_product")
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
    return input_record(build, uses)
