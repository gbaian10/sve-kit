"""Resolve frozen product contents and inclusions through permanent identity gates."""

import re
from dataclasses import dataclass
from typing import TYPE_CHECKING, Literal

from pydantic import JsonValue

from sve_carddb.build_inputs import SourceUse
from sve_carddb.products.identity_models import ExpansionLink, ProductLink
from sve_carddb.products.models import InclusionData, LocalizedText, ProductData
from sve_carddb.products.official import date_fields
from sve_carddb.registry.records import PrintingData
from sve_carddb.snapshot.values import canonical

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.products.identities import ProductIdentities
    from sve_carddb.products.official import ProductBlock, ProductPage
    from sve_carddb.registry.preview import PreviewPlan
    from sve_carddb.registry.preview.plan import Projection


Distribution = Literal["pack", "other"]


@dataclass(frozen=True)
class ObservedProduct:
    data: ProductData
    page: ProductPage
    block: ProductBlock


@dataclass(frozen=True)
class ObservedInclusion:
    data: InclusionData
    page: ProductPage
    block: ProductBlock


@dataclass(frozen=True)
class OfficialProducts:
    identities: ProductIdentities
    pages: tuple[ProductPage, ...]
    products: tuple[ObservedProduct, ...]
    inclusions: tuple[ObservedInclusion, ...]
    diagnostics: tuple[JsonValue, ...]
    preview: PreviewPlan

    def source_uses(self) -> tuple[SourceUse, ...]:
        """Declare actual reads, including unadopted blocks and pages with no blocks."""
        return (
            *self.identities.source_uses(),
            *(
                SourceUse(
                    source=page.source,
                    usage="official_product_page",
                    locator=canonical(
                        {"region": page.region, "card_no": page.card_no}
                    ).decode(),
                )
                for page in self.pages
            ),
            *(
                SourceUse(source=page.source, usage=usage, locator=block.locator)
                for page in self.pages
                for block in page.blocks
                for usage in ("official_product", "official_printing_product")
            ),
        )

    def report(self) -> dict[str, JsonValue]:
        """Report exact product observations and exclusions, without card effect text."""
        return {
            "source_pages": len(self.pages),
            "source_blocks": sum(len(page.blocks) for page in self.pages),
            "identity_records": len(self.identities.records),
            "products": len(self.products),
            "inclusions": len(self.inclusions),
            "warnings": list(self.identities.warnings),
            "diagnostics": list(self.diagnostics),
            "observations": [
                {
                    "source_id": page.source.id,
                    "region": page.region,
                    "card_no": page.card_no,
                    "blocks": [
                        {
                            "locator": block.locator,
                            "name": block.name,
                            "date_raw": block.date_raw,
                            "href_raw": list[JsonValue](block.href_raw),
                            "resolved_urls": list[JsonValue](block.resolved_urls),
                            "matches": [
                                match.model_dump(mode="json") for match in block.matches
                            ],
                        }
                        for block in page.blocks
                    ],
                }
                for page in self.pages
            ],
        }


class ProductConflictError(ValueError):
    def __init__(
        self, product_id: str, observations: tuple[ObservedProduct, ...]
    ) -> None:
        self.report: dict[str, JsonValue] = {
            "reason": "conflicting_official_product_content",
            "product_id": product_id,
            "observations": [
                {
                    "source_id": item.page.source.id,
                    "locator": item.block.locator,
                    "data": item.data.model_dump(mode="json"),
                }
                for item in observations
            ],
        }
        super().__init__("Conflicting official product content: " + product_id)


def _source_type(name: str) -> tuple[str | None, Distribution | None]:
    # Recognized nouns come from this block's title, never its owner or family kind.
    labels: tuple[tuple[str, str, Distribution], ...] = (
        (r"パック|\b(?:Booster|Special) Pack\b", "pack", "pack"),
        (
            r"デッキ|\b(?:Starter|Trial|Prebuilt|Showdown) Deck\b",
            "deck",
            "other",
        ),
        (
            r"\b(?:Booster|Crossover|Starter|Leader Card|Premium Card|Special|Combined) Set\b|カードセット",
            "set",
            "other",
        ),
        (r"\bBundle\b", "bundle", "other"),
    )
    found = {
        (kind, inclusion)
        for pattern, kind, inclusion in labels
        if re.search(pattern, name) is not None
    }
    if len(found) != 1:
        return None, None
    return next(iter(found))


def _product(
    identifier: str, page: ProductPage, block: ProductBlock
) -> tuple[ObservedProduct | None, Distribution | None]:
    kind, inclusion = _source_type(block.name)
    if kind is None:
        return None, None
    released, precision = date_fields(block.date_raw)
    codes = {
        match.expansion_code
        for match in block.matches
        if isinstance(match, (ProductLink, ExpansionLink))
        and match.expansion_code is not None
    }
    data = ProductData(
        id=identifier,
        region=page.region,
        family_id=None,
        product_code=next(iter(codes)) if len(codes) == 1 else None,
        name=LocalizedText(lang="ja" if page.region == "jp" else "en", text=block.name),
        product_type=kind,
        released_on=released,
        date_precision=precision,
        date_raw=block.date_raw,
    )
    return ObservedProduct(data, page, block), inclusion


def _diagnostic(
    page: ProductPage,
    block: ProductBlock,
    reason: str,
    identifier: str | None = None,
    printing_id: str | None = None,
) -> dict[str, JsonValue]:
    exclusion = (
        "product_and_inclusion"
        if reason
        in {
            "missing_product_identity",
            "product_type_or_distribution_unavailable",
            "outside_output_regions",
        }
        else "inclusion"
        if reason.startswith("printing_gate:")
        or reason in {"printing_source_mismatch", "unregistered_printing"}
        else None
    )
    return {
        "reason": reason,
        "excludes": exclusion,
        "source_id": page.source.id,
        "region": page.region,
        "card_no": page.card_no,
        "locator": block.locator,
        "matches": [match.model_dump(mode="json") for match in block.matches],
        "product_id": identifier,
        "printing_id": printing_id,
    }


def plan_official_products(
    identities: ProductIdentities, pages: tuple[ProductPage, ...], preview: PreviewPlan
) -> OfficialProducts:
    """Select only explicitly observed contents and already eligible exact printings."""
    _check_observation_conflicts(identities, pages)
    products: dict[str, list[ObservedProduct]] = {}
    inclusions: dict[tuple[str, str], list[ObservedInclusion]] = {}
    diagnostics: list[JsonValue] = []
    printings: dict[tuple[str, str], list[PrintingData]] = {}
    for record in preview.snapshot.records.values():
        if isinstance(record.data, PrintingData):
            printings.setdefault((record.data.region, record.data.card_no), []).append(
                record.data
            )
    projections = {item.record_key: item for item in preview.projections}
    for page in pages:
        for block in page.blocks:
            identifier = identities.match(page.region, block.matches)
            diagnostics.extend(
                _diagnostic(page, block, reason, identifier)
                for reason in block.diagnostics
            )
            if identifier is None:
                diagnostics.append(_diagnostic(page, block, "missing_product_identity"))
                continue
            observed, inclusion_kind = _product(identifier, page, block)
            if observed is None or inclusion_kind is None:
                diagnostics.append(
                    _diagnostic(
                        page,
                        block,
                        "product_type_or_distribution_unavailable",
                        identifier,
                    )
                )
                continue
            if page.region not in preview.regions:
                diagnostics.append(
                    _diagnostic(page, block, "outside_output_regions", identifier)
                )
                continue
            if observed.data.date_precision == "unknown":
                diagnostics.append(
                    _diagnostic(page, block, "product_date_unknown", identifier)
                )
            products.setdefault(identifier, []).append(observed)
            found = printings.get((page.region, page.card_no), [])
            if not found:
                diagnostics.append(
                    _diagnostic(page, block, "unregistered_printing", identifier)
                )
            _inclusions(
                preview,
                page,
                block,
                identifier,
                inclusion_kind,
                found,
                projections=projections,
                inclusions=inclusions,
                diagnostics=diagnostics,
            )
    return OfficialProducts(
        identities,
        pages,
        _deduplicate_products(products),
        _deduplicate_inclusions(inclusions),
        tuple(diagnostics),
        preview,
    )


def _inclusions(  # ruff: ignore[too-many-arguments] -- shared diagnostic and inclusion accumulators preserve exact source context
    preview: PreviewPlan,
    page: ProductPage,
    block: ProductBlock,
    identifier: str,
    inclusion_kind: Distribution,
    found: list[PrintingData],
    *,
    projections: Mapping[str, Projection],
    inclusions: dict[tuple[str, str], list[ObservedInclusion]],
    diagnostics: list[JsonValue],
) -> None:
    for printing in found:
        projection = projections["printing:" + printing.id]
        if projection.disposition != "included":
            diagnostics.extend(
                _diagnostic(
                    page,
                    block,
                    "printing_gate:" + reason,
                    identifier,
                    printing.id,
                )
                for reason in projection.reasons
            )
            continue
        evidence = preview.evidence.get((page.region, page.card_no))
        if evidence is None or evidence.source.values() != page.source.values():
            diagnostics.append(
                _diagnostic(
                    page,
                    block,
                    "printing_source_mismatch",
                    identifier,
                    printing.id,
                )
            )
            continue
        data = InclusionData(
            printing_id=printing.id,
            product_id=identifier,
            first_available_on=None,
            first_available_precision=None,
            first_available_raw=None,
            inclusion_kind=inclusion_kind,
            note=None,
        )
        inclusions.setdefault((printing.id, identifier), []).append(
            ObservedInclusion(data, page, block)
        )


def _check_observation_conflicts(
    identities: ProductIdentities, pages: tuple[ProductPage, ...]
) -> None:
    groups: dict[str, list[tuple[ProductPage, ProductBlock]]] = {}
    for page in pages:
        for block in page.blocks:
            identifier = identities.match(page.region, block.matches)
            if identifier is not None:
                groups.setdefault(identifier, []).append((page, block))
    for identifier, group in sorted(groups.items()):
        if (
            len({(page.region, block.name, block.date_raw) for page, block in group})
            > 1
        ):
            raise ProductObservationConflictError(identifier, group)


class ProductObservationConflictError(ValueError):
    def __init__(
        self, identifier: str, observations: list[tuple[ProductPage, ProductBlock]]
    ) -> None:
        self.report: dict[str, JsonValue] = {
            "reason": "conflicting_official_product_observation",
            "product_id": identifier,
            "observations": [
                {
                    "source_id": page.source.id,
                    "locator": block.locator,
                    "region": page.region,
                    "name": block.name,
                    "date_raw": block.date_raw,
                }
                for page, block in sorted(
                    observations, key=lambda item: (item[0].source.id, item[1].ordinal)
                )
            ],
        }
        super().__init__("Conflicting official product observation: " + identifier)


def _deduplicate_products(
    groups: dict[str, list[ObservedProduct]],
) -> tuple[ObservedProduct, ...]:
    result: list[ObservedProduct] = []
    for identifier, group in sorted(groups.items()):
        if len({item.data for item in group}) != 1:
            raise ProductConflictError(identifier, tuple(group))
        result.append(
            min(group, key=lambda item: (item.page.source.id, item.block.ordinal))
        )
    return tuple(result)


def _deduplicate_inclusions(
    groups: dict[tuple[str, str], list[ObservedInclusion]],
) -> tuple[ObservedInclusion, ...]:
    result: list[ObservedInclusion] = []
    for group in groups.values():
        if len({item.data for item in group}) != 1:
            raise ValueError("Conflicting official printing/product inclusion")
        result.append(
            min(group, key=lambda item: (item.page.source.id, item.block.ordinal))
        )
    return tuple(
        sorted(result, key=lambda item: (item.data.printing_id, item.data.product_id))
    )
