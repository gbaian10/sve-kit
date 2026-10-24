"""Verify both vendors' exact reports and the backed-up review input closure."""

from pathlib import Path
from typing import TYPE_CHECKING, Literal

from pydantic import field_validator

from sve_carddb.core.authored import check_path
from sve_carddb.core.json import canonical, digest, object_value, parse
from sve_carddb.core.models import Hash, RecordData, Text
from sve_carddb.core.provenance import FilePin
from sve_carddb.domains.construction.adoption_models import ReviewReport, ordered

if TYPE_CHECKING:
    from sve_carddb.domains.construction.adoption_loader import Snapshot

REQUIRED_DEPENDENCIES = (
    "carddb/pyproject.toml",
    "carddb/src/sve_carddb/core/provenance.py",
    "carddb/src/sve_carddb/domains/construction/adoption_loader.py",
    "carddb/src/sve_carddb/domains/construction/adoption_models.py",
    "carddb/src/sve_carddb/domains/construction/adoption_reports.py",
    "carddb/src/sve_carddb/domains/construction/adoption_sources.py",
    "carddb/src/sve_carddb/domains/construction/adoption_validation.py",
    "carddb/src/sve_carddb/parse/pages/restriction_list.py",
    "carddb/uv.lock",
)


class ToolConfiguration(RecordData):
    recipe: Literal["construction-two-models-v1"]
    scope: Literal["current-list-day-v1"]
    dependencies: tuple[FilePin, ...]
    construction_review_stores: tuple[Text, ...]


class StorePin(RecordData):
    store_id: Text
    root: Path
    backup: Path
    restored: Path
    inventory_sha256: Hash

    def read(self, relative: str) -> bytes:
        """Verify independent report copies before trusting their exact bytes."""
        copies = (self.root, self.backup, self.restored)
        resolved = [root.resolve() for root in copies]
        if any(not root.is_absolute() for root in copies) or any(
            left.is_relative_to(right) or right.is_relative_to(left)
            for i, left in enumerate(resolved)
            for right in resolved[i + 1 :]
        ):
            raise ValueError(
                "Construction reports need disjoint absolute store, backup and restore roots"
            )
        contents = []
        for root in copies:
            path = root / relative
            check_path(root, path)
            contents.append(path.read_bytes())
        if contents[1:] != contents[:1] * 2:
            raise ValueError(
                "Construction report backup or restore differs from primary bytes"
            )
        return contents[0]


class ReportInventory(RecordData):
    construction_review_inventory_format: Literal[1]
    store_id: Text
    reports: tuple[Hash, ...]
    dependencies: tuple[FilePin, ...]
    review_inputs: tuple[FilePin, ...]

    @field_validator("construction_review_inventory_format", mode="before")
    @classmethod
    def integer_format(cls, value: object) -> object:
        """Reject booleans before Literal coercion."""
        if type(value) is not int:
            raise ValueError("Review inventory format must be integer one")
        return value


def report_path(checksum: str) -> str:
    """Locate reports by content hash without embedding machine paths."""
    return "reports/sha256/" + checksum[7:9] + "/" + checksum[7:] + ".json"


def verify_reports(  # ruff: ignore[complex-structure, too-many-branches, too-many-locals] -- Verify the complete report and backup closure before any adoption.
    snapshot: Snapshot, stores: tuple[StorePin, ...]
) -> list[dict[str, str]]:
    """Require matching final-basis reports and their backed-up tool pins."""
    by_name = {store.store_id: store for store in stores}
    if len(by_name) != len(stores):
        raise ValueError("Duplicate construction review store")
    inventories: dict[str, ReportInventory] = {}
    for name, store in by_name.items():
        raw = store.read("inventory.json")
        inventory = ReportInventory.model_validate_json(canonical(parse(raw)))
        if (
            digest(raw) != store.inventory_sha256
            or inventory.store_id != name
            or inventory.reports != tuple(sorted(set(inventory.reports)))
        ):
            raise ValueError(
                "Construction review inventory hash, identity or order mismatch"
            )
        for checksum in inventory.reports:
            if digest(store.read(report_path(checksum))) != checksum:
                raise ValueError("Construction report inventory contains damaged bytes")
        if not inventory.review_inputs or not any(
            pin.name == "inputs/basis-inputs.json" for pin in inventory.review_inputs
        ):
            raise ValueError(
                "Construction reports need their full private basis inputs"
            )
        for pin in inventory.review_inputs:
            if digest(store.read(pin.name)) != pin.sha256:
                raise ValueError("Construction private review input backup mismatch")
        for pin in inventory.dependencies:
            if (
                digest(store.read("dependencies/sha256/" + pin.sha256[7:]))
                != pin.sha256
            ):
                raise ValueError("Construction review dependency backup mismatch")
        ordered(inventory.dependencies)
        ordered(inventory.review_inputs)
        inventories[name] = inventory
    used: set[tuple[str, str]] = set()
    configured: set[str] = set()
    for loaded in snapshot.shards:
        context = loaded.shard.review_context.context
        configuration = ToolConfiguration.model_validate_json(context.configuration)
        if (
            tuple(pin.name for pin in configuration.dependencies)
            != REQUIRED_DEPENDENCIES
        ):
            raise ValueError(
                "Construction review must pin the complete supported tool dependencies"
            )
        if (
            not configuration.construction_review_stores
            or configuration.construction_review_stores
            != tuple(sorted(set(configuration.construction_review_stores)))
        ):
            raise ValueError("Construction review context needs named, sorted stores")
        configured.update(configuration.construction_review_stores)
        for record in loaded.shard.records:
            expected_uses = sorted(
                (item.use.model_dump(mode="json") for item in record.evidence),
                key=canonical,
            )
            for review in record.review.model_reviews:
                matches = [
                    name
                    for name in configuration.construction_review_stores
                    if name in inventories
                    and review.report_hash in inventories[name].reports
                ]
                if len(matches) != 1:
                    raise ValueError(
                        "Construction report needs exactly one declared store with a verified backup"
                    )
                name = matches[0]
                basis_inputs = object_value(
                    parse(by_name[name].read("inputs/basis-inputs.json"))
                )
                final_input = record.model_dump(
                    mode="json", exclude={"review", "adoption_review"}
                )
                if basis_inputs.get(review.basis_hash) != final_input:
                    raise ValueError(
                        "Construction private input differs from the final reviewed basis"
                    )
                report = ReviewReport.model_validate_json(
                    canonical(
                        parse(by_name[name].read(report_path(review.report_hash)))
                    )
                )
                expected = review.model_dump(mode="json", exclude={"report_hash"})
                actual = report.model_dump(
                    mode="json",
                    include={
                        "vendor",
                        "model",
                        "version",
                        "reviewed_at",
                        "basis_hash",
                        "outcome",
                    },
                )
                if (
                    actual != expected
                    or report.program_revision != context.program_revision
                    or report.dependencies != configuration.dependencies
                    or [use.model_dump(mode="json") for use in report.source_uses]
                    != expected_uses
                ):
                    raise ValueError(
                        "Construction report differs from final review basis, source uses or tool pin"
                    )
                if not set(configuration.dependencies).issubset(
                    inventories[name].dependencies
                ):
                    raise ValueError(
                        "Construction report tool dependencies are absent from backup closure"
                    )
                used.add((review.report_hash, name))
    if configured != set(by_name):
        raise ValueError(
            "Construction review store configuration differs from authored context"
        )
    return [
        {"report_hash": checksum, "store_id": name} for checksum, name in sorted(used)
    ]
