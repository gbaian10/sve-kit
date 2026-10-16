"""Verify the permanent registry review input without treating it as official raw."""

from pathlib import Path
from typing import TYPE_CHECKING, Annotated

from pydantic import Field, JsonValue, model_validator

from sve_carddb.core.json import digest, integer, object_value, parse, string
from sve_carddb.core.models import Date, Hash, RecordData, UInt

if TYPE_CHECKING:
    from sve_carddb.domains.registry.records import MappingReviewData

JP_REVIEW_SCOPE = "All Japanese official card extractions in the reviewed input batch"


class ReviewInputPin(RecordData):
    root: Path
    batch_id: Annotated[str, Field(pattern=r"^[A-Za-z0-9][A-Za-z0-9_-]*\Z")]
    inventory_sha256: Hash
    coverage_hash: Hash
    bytes: UInt
    as_of: Date

    @model_validator(mode="after")
    def absolute_root(self) -> ReviewInputPin:
        """The runtime root locates permanent batches, never a relative fallback."""
        if not self.root.is_absolute():
            raise ValueError("Registry review input root must be absolute")
        return self

    def configuration(self) -> dict[str, JsonValue]:
        """Keep the artifact pin, excluding private filesystem and inventory metadata."""
        return self.model_dump(mode="json", exclude={"root"}) | {
            "filename": "jp.jsonl",
            "target_region": "jp",
            "coverage_scope": JP_REVIEW_SCOPE,
        }


class RegistryReviewInputs:
    def __init__(self, pin: ReviewInputPin) -> None:
        self.pin = pin
        root = pin.root.resolve(strict=True)
        batch = (root / pin.batch_id).resolve(strict=True)
        if batch.parent != root:
            raise ValueError("Registry review batch escapes its permanent root")
        inventory = self._read(batch, "inventory.json")
        if digest(inventory) != pin.inventory_sha256:
            raise ValueError("Registry review inventory hash mismatch")
        entry = object_value(
            object_value(object_value(parse(inventory))["files"])["jp.jsonl"]
        )
        if (string(entry["sha256"]), integer(entry["bytes"])) != (
            pin.coverage_hash,
            pin.bytes,
        ):
            raise ValueError("Registry review inventory JP entry differs from pin")
        sums = self._read(batch, "SHA256SUMS").decode("utf-8").splitlines()
        matches = [line.split() for line in sums if line.split()[-1:] == ["jp.jsonl"]]
        if matches != [[pin.coverage_hash.removeprefix("sha256:"), "jp.jsonl"]]:
            raise ValueError("Registry review SHA256SUMS JP entry differs from pin")
        raw = self._read(batch, "jp.jsonl")
        if len(raw) != pin.bytes or digest(raw) != pin.coverage_hash:
            raise ValueError("Registry review JP bytes or hash mismatch")
        numbers: set[str] = set()
        for line in raw.splitlines():
            number = string(object_value(parse(line))["number"])
            if not number or number in numbers:
                raise ValueError(
                    "Registry review requires unique nonempty JP card numbers"
                )
            numbers.add(number)
        if not numbers:
            raise ValueError("Registry review JP input is empty")
        self.card_numbers = frozenset(numbers)

    @staticmethod
    def _read(batch: Path, filename: str) -> bytes:
        path = (batch / filename).resolve(strict=True)
        if path.parent != batch:
            raise ValueError("Registry review file escapes its batch")
        return path.read_bytes()

    def coverage(self, review: MappingReviewData) -> bool:
        """Only the original JP scope and historical date can use these exact bytes."""
        return (
            review.target_region == "jp"
            and review.coverage_scope == JP_REVIEW_SCOPE
            and review.as_of == self.pin.as_of
            and review.coverage_hash == self.pin.coverage_hash
        )
