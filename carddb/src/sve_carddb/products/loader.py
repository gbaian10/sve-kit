"""Validate the complete catalog before any family or regional projection."""

import re
from dataclasses import dataclass
from pathlib import Path
from types import MappingProxyType
from typing import TYPE_CHECKING

from pydantic import JsonValue, ValidationError

from sve_carddb.products.models import (
    CatalogRecord,
    Decision,
    FamilyRecord,
    InclusionRecord,
    Index,
    ProductRecord,
    Shard,
)
from sve_carddb.registry.inputs import canonical, digest
from sve_carddb.registry.records import PrintingData, RecordData
from sve_carddb.registry.storage import read_yaml

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.registry.snapshot import RegistrySnapshot

_PATH = re.compile(
    r"products/(family|product|inclusion)/([A-Za-z0-9_-]+)/([0-9]{3,})\.yaml"
)
_KINDS = {
    "family": "product_family",
    "product": "product",
    "inclusion": "printing_product",
}


@dataclass(frozen=True)
class LoadedShard:
    path: str
    content_hash: str
    content: bytes
    envelope: Shard


@dataclass(frozen=True)
class ProductSnapshot:
    index_content: bytes
    registry_index_content: bytes
    shards: tuple[LoadedShard, ...]
    records: Mapping[str, CatalogRecord]
    decisions: Mapping[str, Decision]

    def report(self) -> dict[str, JsonValue]:
        """Report candidate/adopted identifiers without product names or card text."""
        return {
            "records": [
                {
                    "record_key": record.record_key,
                    "decision_id": shard.envelope.default_decision_id,
                    "state": shard.envelope.decisions[0].state,
                    "disposition": (
                        "confirmed_family"
                        if isinstance(record, FamilyRecord)
                        and shard.envelope.decisions[0].state == "confirmed"
                        else "candidate"
                        if shard.envelope.decisions[0].state == "proposed"
                        else "projection_unimplemented"
                    ),
                }
                for shard in self.shards
                for record in shard.envelope.records
            ]
        }


def load_products(root: Path, *, registry: RegistrySnapshot) -> ProductSnapshot:
    """Read only indexed, stable authored inputs; do not allocate or write anything.

    This validates the complete wire format and global references. Raw evidence
    verification belongs to the projection boundary; this snapshot alone does
    not attest an evidence locator or authorize product/inclusion projection.
    """
    index_path = root / "products/index.yaml"
    _safe_file(root, index_path)
    raw_index = read_yaml(index_path)
    index = _model(Index, raw_index)
    _inventory(root, set(index.includes))
    shards = tuple(
        _load_shard(root, name, checksum)
        for name, checksum in sorted(index.includes.items())
    )
    records: dict[str, CatalogRecord] = {}
    decisions: dict[str, Decision] = {}
    for shard in shards:
        decision = shard.envelope.decisions[0]
        if decision.id in decisions:
            raise ValueError("Duplicate product decision")
        decisions[decision.id] = decision
        for record in shard.envelope.records:
            if record.record_key in records:
                raise ValueError("Duplicate product record or data primary key")
            records[record.record_key] = record
    _references(records, registry)
    return ProductSnapshot(
        canonical(raw_index),
        registry.files.index_content,
        shards,
        MappingProxyType(records),
        MappingProxyType(decisions),
    )


def _model[T: RecordData](model: type[T], raw: JsonValue) -> T:
    try:
        return model.model_validate_json(canonical(raw))
    except ValidationError:
        # Validation errors may include source text; expose only the boundary name.
        raise ValueError("Invalid product authored fields") from None


def _safe_file(root: Path, path: Path) -> None:
    if not path.is_relative_to(root):
        raise ValueError("Unsafe product path")
    for component in (path, *path.parents):
        if component.is_symlink():
            raise ValueError("Symlinks are forbidden in product inputs")
        if component == root:
            break
    if not path.is_file():
        raise ValueError("Missing product input file")


def _inventory(root: Path, includes: set[str]) -> None:
    for name in includes:
        if _PATH.fullmatch(name) is None:
            raise ValueError("Unsafe product include path")
    directory = root / "products"
    present: set[str] = set()
    for file in directory.rglob("*"):
        if file.is_symlink():
            raise ValueError("Symlinks are forbidden in product inputs")
        if (
            file.suffix.lower() in {".yaml", ".yml"}
            and file != directory / "index.yaml"
        ):
            present.add(file.relative_to(root).as_posix())
    if present != includes:
        raise ValueError("Product indexed file closure differs from disk")


def _load_shard(root: Path, name: str, checksum: str) -> LoadedShard:
    path = root / name
    _safe_file(root, path)
    raw = read_yaml(path)
    if digest(raw) != checksum:
        raise ValueError("Modified immutable product shard")
    shard = _model(Shard, raw)
    _check_records(shard, name)
    _check_decision(shard)
    return LoadedShard(name, checksum, canonical(raw), shard)


def _check_records(shard: Shard, path: str) -> None:
    area, filing = Path(path).parts[1:3]
    keys = [record.record_key for record in shard.records]
    if keys != sorted(set(keys)):
        raise ValueError("Product records must be sorted and unique")
    for record in shard.records:
        if record.kind != _KINDS[area] or record.filing_key != filing:
            raise ValueError("Product record kind/filing key disagrees with path")
        primary: list[JsonValue] = [record.kind]
        if isinstance(record, InclusionRecord):
            primary.extend((record.data.printing_id, record.data.product_id))
        else:
            primary.append(record.data.id)
        if canonical(primary).decode() != record.record_key:
            raise ValueError("Product record key disagrees with data primary key")
        if len(set(record.evidence)) != len(record.evidence):
            raise ValueError("Duplicate product evidence")
        if not isinstance(record, FamilyRecord) and not record.evidence:
            raise ValueError("Product/inclusion evidence must be nonempty")


def _check_decision(shard: Shard) -> None:
    decision = shard.decisions[0]
    members = tuple(
        (record.record_key, digest(record.model_dump(mode="json")))
        for record in shard.records
    )
    checksum = digest([[key, value] for key, value in members])
    if decision.members != members:
        raise ValueError("Product decision exact members disagree")
    if decision.membership_hash != checksum:
        raise ValueError("Product decision membership hash disagrees")
    if decision.id != "d:" + checksum.removeprefix("sha256:"):
        raise ValueError("Product decision ID disagrees with membership hash")
    if shard.default_decision_id != decision.id:
        raise ValueError("Product default decision ID disagrees")
    if decision.state == "confirmed" and decision.sample_ids != tuple(
        key for key, _ in members
    ):
        raise ValueError("Confirmed product decision must check every exact member")


def _references(records: dict[str, CatalogRecord], registry: RegistrySnapshot) -> None:
    families = {
        record.data.id: record
        for record in records.values()
        if isinstance(record, FamilyRecord)
    }
    for field in ("code", "public_code"):
        values = [getattr(record.data, field) for record in families.values()]
        if len(values) != len(set(values)):
            raise ValueError("Duplicate product family code/public_code")
    products = {
        record.data.id: record.data
        for record in records.values()
        if isinstance(record, ProductRecord)
    }
    printings = {
        record.data.id: record.data
        for record in registry.records.values()
        if isinstance(record.data, PrintingData)
    }
    for record in records.values():
        if isinstance(record, ProductRecord):
            if (
                record.data.family_id is not None
                and record.data.family_id not in families
            ):
                raise ValueError("Missing referenced product family")
        elif isinstance(record, InclusionRecord):
            data = record.data
            if data.product_id not in products or data.printing_id not in printings:
                raise ValueError("Missing referenced product/printing")
            if products[data.product_id].region != printings[data.printing_id].region:
                raise ValueError("Product/inclusion printing regions disagree")
