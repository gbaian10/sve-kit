"""Validate all confirmed product IDs, pinned authored bytes and frozen evidence."""

import re
import shutil
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- verify exact authored bytes against an immutable Git object
from dataclasses import dataclass
from pathlib import Path
from types import MappingProxyType
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build_inputs import BuildContext, Source, SourceUse
from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.products.identity_models import (
    ExpansionLink,
    IdentityIndex,
    IdentityRecord,
    IdentityShard,
    ProductLink,
    SourceBlock,
)
from sve_carddb.products.loader import _model, _safe_file
from sve_carddb.products.models import Evidence, ProductRecord
from sve_carddb.products.official import PARSER, ProductPage, parse_products
from sve_carddb.registry.inputs import canonical, digest
from sve_carddb.registry.storage import read_yaml
from sve_carddb.snapshot.values import digest as digest_bytes
from sve_carddb.snapshot.values import parse

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.products.identity_models import Match
    from sve_carddb.products.loader import ProductSnapshot
    from sve_carddb.registry.records import Region

_PATH = re.compile(r"product-identities/(jp|en)/([0-9]{3,})\.yaml")
INDEX_PATH = "product-identities/index.yaml"
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
    index_hash: str
    index_bytes: bytes
    shards: tuple[IdentityFile, ...]
    records: Mapping[str, IdentityRecord]
    evidence: Mapping[Evidence, IdentityEvidence]
    warnings: tuple[JsonValue, ...]

    def dependencies(self) -> dict[str, bytes]:
        """Pin every historical alias shard's exact bytes, including the index."""
        return {"authored/" + INDEX_PATH: self.index_bytes} | {
            "authored/" + shard.path: shard.exact_bytes for shard in self.shards
        }

    def configuration(self) -> dict[str, JsonValue]:
        """Declare the F1 configuration entry specified in authored-layout §11.4."""
        return {
            "authored_revision": self.revision,
            "index_path": INDEX_PATH,
            "index_hash": self.index_hash,
        }

    def verify_context(self, build: BuildContext) -> None:
        """Fail if physical or semantic authored pins differ from the validated input."""
        configuration = parse(build.configuration.encode())
        if (
            not isinstance(configuration, dict)
            or configuration.get("product_identity") != self.configuration()
        ):
            raise ValueError("Product identity configuration pin mismatch")
        pins = {pin.name: pin.sha256 for pin in build.dependencies}
        if any(
            pins.get(name) != digest_bytes(content)
            for name, content in self.dependencies().items()
        ):
            raise ValueError("Product identity dependency pin mismatch")

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


def _revision(root: Path, name: str, revision: str, content: bytes) -> None:
    executable = shutil.which("git")
    if executable is None:
        raise ValueError("Git is required to verify product identity revision")
    result = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- invoke Git without a shell on validated object paths
        [executable, "-C", str(root.parent), "show", revision + ":authored/" + name],
        capture_output=True,
        check=False,
    )
    if result.returncode != 0 or result.stdout != content:
        raise ValueError("Product identity bytes differ from pinned authored revision")


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
    _safe_file(root, root / INDEX_PATH)
    raw = read_yaml(root / INDEX_PATH)
    index = _model(IdentityIndex, raw)
    _inventory(root, set(index.includes))
    shards = tuple(
        _shard(root, name, checksum)
        for name, checksum in sorted(index.includes.items())
    )
    records = _records(shards, catalog)
    index_bytes = (root / INDEX_PATH).read_bytes()
    _revision(root, INDEX_PATH, authored_revision, index_bytes)
    for shard in shards:
        _revision(root, shard.path, authored_revision, shard.exact_bytes)
    pages = _evidence(records, stores)
    return ProductIdentities(
        authored_revision,
        digest(raw),
        index_bytes,
        shards,
        MappingProxyType(records),
        MappingProxyType(pages),
        _warnings(records),
    )


def _inventory(root: Path, includes: set[str]) -> None:
    if any(_PATH.fullmatch(name) is None for name in includes):
        raise ValueError("Unsafe product identity include path")
    directory = root / "product-identities"
    present: set[str] = set()
    for path in directory.rglob("*"):
        if path.is_symlink():
            raise ValueError("Symlinks are forbidden in product identity inputs")
        if (
            path.suffix.lower() in {".yaml", ".yml"}
            and path != directory / "index.yaml"
        ):
            present.add(path.relative_to(root).as_posix())
    if present != includes:
        raise ValueError("Product identity indexed file closure differs from disk")


def _shard(root: Path, name: str, checksum: str) -> IdentityFile:
    path = root / name
    _safe_file(root, path)
    raw = read_yaml(path)
    if digest(raw) != checksum:
        raise ValueError("Modified immutable product identity shard")
    envelope = _model(IdentityShard, raw)
    keys = tuple(record.record_key for record in envelope.records)
    if keys != tuple(sorted(set(keys))):
        raise ValueError("Product identity records must be sorted and unique")
    for record in envelope.records:
        if (
            record.filing_key != Path(name).parts[1]
            or record.filing_key != record.data.region
        ):
            raise ValueError("Product identity filing/path/region mismatch")
        if record.record_key != match_key(record.data.region, record.data.match):
            raise ValueError("Product identity record key mismatch")
        if len(set(record.evidence)) != len(record.evidence):
            raise ValueError("Duplicate product identity evidence")
        if not any(ref.role == "product_identity_match" for ref in record.evidence):
            raise ValueError("Product identity requires match evidence")
    _decision(envelope, keys)
    return IdentityFile(name, checksum, path.read_bytes(), envelope)


def _decision(envelope: IdentityShard, keys: tuple[str, ...]) -> None:
    decision = envelope.decisions[0]
    members = tuple(
        (record.record_key, digest(record.model_dump(mode="json")))
        for record in envelope.records
    )
    membership_hash = digest([[key, checksum] for key, checksum in members])
    if decision.members != members:
        raise ValueError("Product identity exact members mismatch")
    if decision.membership_hash != membership_hash:
        raise ValueError("Product identity membership hash mismatch")
    if decision.id != "d:" + membership_hash.removeprefix("sha256:"):
        raise ValueError("Product identity decision ID mismatch")
    if envelope.default_decision_id != decision.id:
        raise ValueError("Product identity default decision mismatch")
    if decision.sample_ids != keys:
        raise ValueError("Product identity requires every exact member checked")


def _records(
    shards: tuple[IdentityFile, ...], catalog: ProductSnapshot
) -> dict[str, IdentityRecord]:
    records: dict[str, IdentityRecord] = {}
    regions = {
        record.data.id: record.data.region
        for record in catalog.records.values()
        if isinstance(record, ProductRecord)
    }
    decisions: set[str] = set()
    for shard in shards:
        identifier = shard.envelope.decisions[0].id
        if identifier in decisions:
            raise ValueError("Duplicate product identity decision")
        decisions.add(identifier)
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
    batches: dict[tuple[str, str], FrozenSources] = {}
    parsed: dict[tuple[str, str, str], ProductPage] = {}
    pages: dict[Evidence, IdentityEvidence] = {}
    for record in records.values():
        for ref in record.evidence:
            key = ref.store_id, ref.batch_id
            if key not in batches:
                root = stores.get(ref.store_id)
                if root is None:
                    raise ValueError(
                        "Product identity evidence requires an explicit store"
                    )
                batches[key] = FrozenSources(root, *key)
            parse_key = (*key, ref.source_version_id)
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
                if (
                    ordinal >= len(page.blocks)
                    or record.data.match not in page.blocks[ordinal].matches
                ):
                    raise ValueError(
                        "Product identity evidence cannot reproduce exact match"
                    )
                if isinstance(record.data.match, SourceBlock) and (
                    record.data.match.source_version_id,
                    record.data.match.product_block_ordinal,
                ) != (ref.source_version_id, ordinal):
                    raise ValueError("Product identity source block evidence mismatch")
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
