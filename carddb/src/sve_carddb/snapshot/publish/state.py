"""Private backed release ledger, reservation receipts and recovery evidence."""

import fcntl
import os
from contextlib import contextmanager
from copy import deepcopy
from dataclasses import dataclass
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.snapshot.publish.storage import PublishError
from sve_carddb.snapshot.values import (
    SAFE_INTEGER,
    array,
    canonical,
    digest,
    integer,
    object_value,
    parse,
    string,
)

if TYPE_CHECKING:
    from collections.abc import Iterator
    from pathlib import Path


def atomic(path: Path, raw: bytes) -> None:
    """Fsync both file contents and directory entries before acknowledging state."""
    temporary = path.with_suffix(".pending")
    with temporary.open("wb") as stream:
        stream.write(raw)
        stream.flush()
        os.fsync(stream.fileno())
    temporary.replace(path)
    descriptor = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def _records(raw: bytes) -> list[dict[str, JsonValue]]:
    result = [object_value(parse(line)) for line in raw.splitlines()]
    versions: set[str] = set()
    for number, item in enumerate(result, start=1):
        if (
            set(item) != {"revision", "data_version"}
            or integer(item["revision"]) != number
            or not string(item["data_version"])
            or string(item["data_version"]) in versions
        ):
            raise PublishError("Invalid complete reservation receipts")
        versions.add(string(item["data_version"]))
    return result


def checked(raw: bytes, receipts: bytes) -> dict[str, JsonValue]:
    """Recovery needs a complete reservation chain, not the public window's max v."""
    state = object_value(parse(raw))
    reservations = _records(receipts)
    if (
        set(state) != {"format", "high_water", "attempts", "text_keys", "first_events"}
        or state["format"] != 1
        or type(state["format"]) is not int
        or integer(state["high_water"]) != len(reservations)
    ):
        raise PublishError("Release state and reservation evidence disagree")
    attempts = array(state["attempts"])
    if len(attempts) != len(reservations):
        raise PublishError("Release state has incomplete reservation history")
    for attempt, reservation in zip(attempts, reservations, strict=True):
        item = object_value(attempt)
        valid_status = item["status"] in {
            "reserved",
            "sealed",
            "failed",
            "committed",
            "superseded",
        }
        incomplete = (item["status"] != "reserved" and item["plan"] is None) or (
            item["status"] == "committed" and item["receipt"] is None
        )
        if (
            set(item) != {"revision", "data_version", "status", "plan", "receipt"}
            or any(item[k] != reservation[k] for k in reservation)
            or not valid_status
            or incomplete
        ):
            raise PublishError("Invalid release attempt evidence")
    object_value(state["text_keys"])
    object_value(state["first_events"])
    return state


@dataclass(frozen=True)
class Checkpoint:
    """Backup-workflow evidence retained separately from a restored backup copy."""

    high_water: int
    state_sha256: str
    reservations_sha256: str


class Ledger:
    """Repo-external state and independently durable backup, excluded from GC.

    Receipt storage is append-only and precedes the main state mutation. A crash
    between these writes fails closed; it never silently guesses a high-water
    mark. Backup loss/mismatch blocks external publication, including retries.
    """

    def __init__(self, root: Path, backup: Path) -> None:
        self.root = root.resolve()
        self.backup = backup.resolve()
        if (
            self.root == self.backup
            or self.root in self.backup.parents
            or self.backup in self.root.parents
        ):
            raise PublishError("Release state and backup roots must be disjoint")
        for path in (self.root, self.backup):
            path.mkdir(parents=True, exist_ok=True)
        self.path = self.root / "release-state.json"
        self.copy = self.backup / "release-state.json"
        self.receipts = self.backup / "reservations.jsonl"

    @contextmanager
    def exclusive(self) -> Iterator[None]:
        """Serialize allocation, publication and receipts, including recovery."""
        with (self.root / "writer.lock").open("ab") as stream:
            fcntl.flock(stream.fileno(), fcntl.LOCK_EX)
            yield

    def initialize(self) -> None:
        """Explicit first-ever setup refuses any existing state or recovery evidence."""
        with self.exclusive():
            if any(p.exists() for p in (self.path, self.copy, self.receipts)):
                raise PublishError("Release state already exists; recover instead")
            atomic(self.receipts, b"")
            self.save(
                {
                    "format": 1,
                    "high_water": 0,
                    "attempts": [],
                    "text_keys": {},
                    "first_events": {},
                }
            )

    def read(self) -> dict[str, JsonValue]:
        """Missing primary state never means a fresh allocator."""
        if not self.path.exists() or not self.receipts.exists():
            raise PublishError("Release state missing; verified recovery required")
        return checked(self.path.read_bytes(), self.receipts.read_bytes())

    def save(self, state: dict[str, JsonValue]) -> None:
        """A read-back verified durable backup is required before external use."""
        raw = canonical(state)
        checked(raw, self.receipts.read_bytes())
        atomic(self.path, raw)
        atomic(self.copy, raw)
        if self.copy.read_bytes() != raw:
            raise PublishError("Release backup verification failed")

    def verify_backup(self) -> None:
        """Keep the backup gate explicit at every external-I/O boundary."""
        self.read()
        if not self.copy.exists() or self.copy.read_bytes() != self.path.read_bytes():
            raise PublishError("Release backup is missing or stale")

    def reserve(self, data_version: str) -> int:
        """Burn a positive safe revision even if later generation/upload fails."""
        with self.exclusive():
            state = self.read()
            if not data_version or data_version.startswith("preview-"):
                raise PublishError("Formal reservation requires a data version")
            if any(
                object_value(a)["data_version"] == data_version
                for a in array(state["attempts"])
            ):
                raise PublishError("Data version was already reserved")
            number = integer(state["high_water"]) + 1
            if number > SAFE_INTEGER:
                raise PublishError("Release revision domain exhausted")
            record: dict[str, JsonValue] = {
                "revision": number,
                "data_version": data_version,
            }
            with self.receipts.open("ab") as stream:
                stream.write(canonical(record) + b"\n")
                stream.flush()
                os.fsync(stream.fileno())
            state["high_water"] = number
            array(state["attempts"]).append(
                record | {"status": "reserved", "plan": None, "receipt": None}
            )
            self.save(state)
            self.verify_backup()
            return number

    def checkpoint(self) -> Checkpoint:
        """Pin verified complete evidence before loss, never infer it after loss."""
        with self.exclusive():
            self.verify_backup()
            return Checkpoint(
                integer(self.read()["high_water"]),
                digest(self.path.read_bytes()),
                digest(self.receipts.read_bytes()),
            )

    def recover(self, *, observed_max: int, proof: Checkpoint) -> None:
        """Restore only the verified backup plus all failed/unpublished reservations."""
        with self.exclusive():
            if self.path.exists():
                raise PublishError("Recovery refuses to replace existing release state")
            if not self.copy.exists() or not self.receipts.exists():
                raise PublishError("Recovery requires backup and complete receipts")
            raw = self.copy.read_bytes()
            if (
                digest(raw) != proof.state_sha256
                or digest(self.receipts.read_bytes()) != proof.reservations_sha256
            ):
                raise PublishError(
                    "Recovery backup differs from independently pinned checkpoint"
                )
            state = checked(raw, self.receipts.read_bytes())
            if integer(state["high_water"]) != integer(proof.high_water):
                raise PublishError(
                    "Recovery checkpoint does not prove the high-water mark"
                )
            if integer(observed_max) < 0 or integer(state["high_water"]) < observed_max:
                raise PublishError("Recovery evidence is below observed image versions")
            atomic(self.path, raw)
            self.verify_backup()

    def media_basis(self) -> JsonValue:
        """Superseded writes force new tokens even when restoring committed bytes."""
        with self.exclusive():
            state = self.read()
            return media_basis(state)

    def supersede(self, revision: int) -> None:
        """Abandon an unfinished plan; the next release must reverify every image."""
        with self.exclusive():
            state = self.read()
            item = attempt(state, revision)
            if item["status"] not in {"sealed", "failed"}:
                raise PublishError(
                    "Only an unfinished sealed release can be superseded"
                )
            item["status"] = "superseded"
            self.save(state)


def attempt(state: dict[str, JsonValue], revision: int) -> dict[str, JsonValue]:
    """Reject guessed/bool revisions before indexing a private reservation."""
    number = integer(revision)
    if not 0 < number <= integer(state["high_water"]):
        raise PublishError("Unknown release reservation")
    return object_value(array(state["attempts"])[number - 1])


def merge_text_keys(state: dict[str, JsonValue], texts: dict[str, str]) -> None:
    """Keep full digests without storing permanent official text or snapshots."""
    keys = object_value(state["text_keys"])
    for key, full in texts.items():
        if key in keys and keys[key] != full:
            raise PublishError("Published text ID collision")
        keys[key] = full


def state_hash(ledger: Ledger) -> str:
    """Expose only a digest in publication reports."""
    return digest(canonical(ledger.read()))


def media_basis(state: dict[str, JsonValue]) -> JsonValue:
    """Compare to the last committed release, invalidating superseded outputs."""
    result: JsonValue = None
    for raw in array(state["attempts"]):
        item = object_value(raw)
        if item["status"] == "committed":
            result = deepcopy(object_value(item["plan"])["media_state"])
        elif item["status"] == "superseded" and result is not None:
            for member in object_value(object_value(result)["members"]).values():
                object_value(member).update(
                    active=False,
                    card=None,
                    art=None,
                    card_version=None,
                    art_version=None,
                )
    return result
