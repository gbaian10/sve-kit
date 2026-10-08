"""Validate current confirmed product IDs against frozen evidence."""

import re
from dataclasses import dataclass
from pathlib import Path
from types import MappingProxyType
from typing import TYPE_CHECKING

from pydantic import JsonValue, ValidationError

from sve_carddb.core.json import parse
from sve_carddb.core.models import RecordData
from sve_carddb.core.provenance import BuildContext, Source, SourceUse
from sve_carddb.core.yaml import JSON_VALUE, MAX_BYTES, parse_yaml
from sve_carddb.ingest.archive.frozen_sources import FrozenSources
from sve_carddb.products.identity_models import (
    ExpansionLink,
    IdentityRecord,
    IdentityShard,
    ProductLink,
    SourceBlock,
)
from sve_carddb.products.loader import _safe_file
from sve_carddb.products.models import Evidence, ProductRecord
from sve_carddb.products.official import PARSER, ProductPage, parse_products
from sve_carddb.registry.inputs import canonical, digest

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.products.identity_models import Match
    from sve_carddb.products.loader import ProductSnapshot
    from sve_carddb.registry.records import Region

_PATH = re.compile(r"product-identities/(jp|en)/([0-9]{3,})\.yaml")
MAX_ORDINAL = 9_007_199_254_740_991


@dataclass(frozen=True)
class IdentityFile:
    path: str
    checksum: str
    exact_bytes: bytes
    envelope: IdentityShard


@dataclass(frozen=True)
class IdentityEvidence:
    source: Source
    page: ProductPage | None


@dataclass(frozen=True)
class ProductIdentities:
    revision: str
    shards: tuple[IdentityFile, ...]
    records: Mapping[str, IdentityRecord]
    evidence: Mapping[Evidence, IdentityEvidence]
    warnings: tuple[JsonValue, ...]
    catalog: ProductSnapshot

    def configuration(self) -> dict[str, JsonValue]:
        """Declare the F1 configuration entry specified in authored-layout §11.4."""
        return {"authored_revision": self.revision}

    def verify_context(self, build: BuildContext) -> None:
        """Fail if physical or semantic authored pins differ from the validated input."""
        configuration = parse(build.configuration.encode())
        if (
            not isinstance(configuration, dict)
            or configuration.get("product_identity") != self.configuration()
        ):
            raise ValueError("Product identity configuration pin mismatch")

    def source_uses(self) -> tuple[SourceUse, ...]:
        """Declare closure and actual match-reproduction uses from all evidence records."""
        uses: list[SourceUse] = []
        for record in self.records.values():
            for reference in record.evidence:
                checked = self.evidence[reference]
                uses.append(
                    SourceUse(
                        source=checked.source.model_copy(
                            update={"parser_version": "archive-closure-v1"}
                        ),
                        usage="product_identity_evidence_closure",
                        locator=canonical(reference.model_dump(mode="json")).decode(),
                    )
                )
                if reference.role == "product_identity_match":
                    uses.append(
                        SourceUse(
                            source=checked.source,
                            usage="official_product_identity",
                            locator=reference.locator,
                        )
                    )
        return tuple(uses)

    def match(self, region: Region, matches: tuple[Match, ...]) -> str | None:
        """Resolve all exact aliases; distinct permanent targets are a hard conflict."""
        identifiers = {
            record.data.product_id
            for match in matches
            if (record := self.records.get(match_key(region, match))) is not None
        }
        if len(identifiers) > 1:
            raise ValueError("Conflicting permanent product identities")
        return next(iter(identifiers)) if identifiers else None


def match_key(region: Region, match: Match) -> str:
    """Use the complete canonical region/match key, with no name or ID normalization."""
    return canonical(
        ["product_identity", region, match.model_dump(mode="json")]
    ).decode()


def load_product_identities(
    root: Path,
    *,
    authored_revision: str,
    catalog: ProductSnapshot,
    stores: Mapping[str, Path],
) -> ProductIdentities:
    """Validate the entire input before any regional selection, without writes."""
    if re.fullmatch(r"[0-9a-f]{40}", authored_revision) is None:
        raise ValueError("Product identity revision must be a full Git SHA")
    shards = tuple(_shard(root, name) for name in _inventory(root))
    records = _records(shards, catalog)
    pages = _evidence(records, stores)
    return ProductIdentities(
        authored_revision,
        shards,
        MappingProxyType(records),
        MappingProxyType(pages),
        _warnings(records),
        catalog,
    )


def _model[T: RecordData](model: type[T], raw: JsonValue) -> T:
    try:
        return model.model_validate_json(canonical(raw))
    except ValidationError as error:
        # Paths and error codes identify constraints without exposing input text.
        details = "; ".join(
            ".".join(map(str, issue["loc"])) + ":" + issue["type"]
            for issue in error.errors(include_input=False, include_context=False)
        )
        raise ValueError("Invalid product identity fields: " + details) from None


def _inventory(root: Path) -> list[str]:
    directory = root / "product-identities"
    # A missing directory must not read as an empty identity mapping.
    if directory.is_symlink() or not directory.is_dir():
        raise ValueError("Missing product identity input directory")
    names: list[str] = []
    for path in directory.rglob("*"):
        if path.is_symlink():
            raise ValueError("Symlinks are forbidden in product identity inputs")
        if path.suffix.lower() in {".yaml", ".yml"}:
            name = path.relative_to(root).as_posix()
            if _PATH.fullmatch(name) is None:
                raise ValueError("Unexpected product identity input path")
            names.append(name)
    return sorted(names)


def _shard(root: Path, name: str) -> IdentityFile:
    path = root / name
    _safe_file(root, path)
    exact = path.read_bytes()
    if len(exact) >= MAX_BYTES:
        raise ValueError("Product identity shard exceeds size limit")
    raw = JSON_VALUE.validate_python(parse_yaml(exact), strict=True)
    envelope = _model(IdentityShard, raw)
    keys = tuple(record.record_key for record in envelope.records)
    if len(keys) != len(set(keys)):
        raise ValueError("Product identity records must be sorted and unique")
    for record in envelope.records:
        if (
            record.filing_key != Path(name).parts[1]
            or record.filing_key != record.data.region
        ):
            raise ValueError("Product identity filing/path/region mismatch")
        if len(set(record.evidence)) != len(record.evidence):
            raise ValueError("Duplicate product identity evidence")
        if not any(ref.role == "product_identity_match" for ref in record.evidence):
            raise ValueError("Product identity requires match evidence")
    return IdentityFile(name, digest(raw), exact, envelope)


def _records(
    shards: tuple[IdentityFile, ...], catalog: ProductSnapshot
) -> dict[str, IdentityRecord]:
    records: dict[str, IdentityRecord] = {}
    regions = {
        record.data.id: record.data.region
        for record in catalog.records.values()
        if isinstance(record, ProductRecord)
    }
    for shard in shards:
        for record in shard.envelope.records:
            if record.record_key in records:
                raise ValueError("Duplicate global product identity match")
            records[record.record_key] = record
            previous = regions.setdefault(record.data.product_id, record.data.region)
            if previous != record.data.region:
                raise ValueError("Product ID used across regions")
    return records


def _ordinal(locator: str) -> int:
    value = parse(locator.encode())
    if (
        not isinstance(value, dict)
        or set(value) != {"product_block_ordinal"}
        or canonical(value).decode() != locator
    ):
        raise ValueError("Product identity locator must be canonical block JSON")
    ordinal = value["product_block_ordinal"]
    if type(ordinal) is not int or ordinal < 0 or ordinal > MAX_ORDINAL:
        raise ValueError("Invalid product identity block ordinal")
    return ordinal


def _evidence(
    records: Mapping[str, IdentityRecord], stores: Mapping[str, Path]
) -> dict[Evidence, IdentityEvidence]:
    batches: dict[str, FrozenSources] = {}
    parsed: dict[tuple[str, str], ProductPage] = {}
    pages: dict[Evidence, IdentityEvidence] = {}
    for record in records.values():
        for ref in record.evidence:
            key = ref.batch_id
            if key not in batches:
                batches[key] = FrozenSources.configured(stores, key)
            parse_key = (key, ref.source_version_id)
            source, raw, descriptor = batches[key].read(
                ref.source_version_id, parser_version="archive-closure-v1"
            )
            page: ProductPage | None = None
            if ref.role == "product_identity_match":
                if (
                    descriptor.provider != record.data.region
                    or descriptor.kind != "card"
                ):
                    raise ValueError("Product identity source region/kind mismatch")
                source = source.model_copy(update={"parser_version": PARSER})
                if parse_key not in parsed:
                    parsed[parse_key] = parse_products(raw, source, record.data.region)
                page = parsed[parse_key]
                ordinal = _ordinal(ref.locator)
                if isinstance(record.data.match, SourceBlock) and (
                    record.data.match.source_version_id,
                    record.data.match.product_block_ordinal,
                ) != (ref.source_version_id, ordinal):
                    raise ValueError("Product identity source block evidence mismatch")
                if (
                    ordinal >= len(page.blocks)
                    or record.data.match not in page.blocks[ordinal].matches
                ):
                    raise ValueError(
                        "Product identity evidence cannot reproduce exact match"
                    )
            pages[ref] = IdentityEvidence(source, page)
    return pages


def _warnings(records: Mapping[str, IdentityRecord]) -> tuple[JsonValue, ...]:
    groups: dict[tuple[str, str], list[IdentityRecord]] = {}
    for record in records.values():
        match = record.data.match
        if (
            isinstance(match, (ProductLink, ExpansionLink))
            and match.expansion_code is not None
        ):
            groups.setdefault((record.data.region, match.expansion_code), []).append(
                record
            )
    return tuple(
        {
            "reason": "expansion_multiple_product_ids",
            "region": region,
            "expansion_code": code,
            "records": [record.model_dump(mode="json") for record in group],
        }
        for (region, code), group in sorted(groups.items())
        if len({record.data.product_id for record in group}) > 1
    )
