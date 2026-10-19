"""Read admitted announcement fragments without HTML or repeat admission checks."""

from typing import TYPE_CHECKING, Literal

from pydantic import JsonValue, ValidationError

from sve_carddb.contracts.snapshot import validate as validate_public
from sve_carddb.core.json import digest, parse
from sve_carddb.core.models import Date, Hash, Instant, RecordData, Text
from sve_carddb.core.provenance import ArchivePin, Source, Version
from sve_carddb.core.regions import Region
from sve_carddb.domains.card_extras.models import (
    ErrataChange,
    ErrataPage,
    ErrataPrinting,
)
from sve_carddb.domains.parsed_cards.trusted import decode

if TYPE_CHECKING:
    from pathlib import Path


class Evidence(RecordData):
    id: Version
    url: Text
    sha256: Hash
    fetched_at: Instant
    etag: str | None
    last_modified: str | None
    parser_version: Text
    batch_id: Hash
    descriptor_sha256: Hash
    first_receipt_id: Hash

    def source(self, store_id: str) -> Source:
        """Bind portable evidence to the caller's private archive identifier."""
        checksum = self.sha256.removeprefix("sha256:")
        return Source.model_construct(
            id=self.id,
            kind="official_page",
            url=self.url,
            sha256=self.sha256,
            fetched_at=self.fetched_at,
            etag=self.etag,
            last_modified=self.last_modified,
            parser_version=self.parser_version,
            raw_locator=f"{store_id}:raw/sha256/{checksum[:2]}/{checksum}.raw",
            archive=ArchivePin.model_construct(
                store_id=store_id,
                batch_id=self.batch_id,
                descriptor_sha256=self.descriptor_sha256,
                first_receipt_id=self.first_receipt_id,
            ),
        )


class Slice(RecordData):
    start: int
    end: int
    sha256: Hash


class Block(RecordData):
    ordinal: int
    context: tuple[str, ...]
    before: str
    after: str
    raw_before: Slice
    raw_after: Slice


class Change(RecordData):
    card_no: Text
    face_id: Text
    field: Text
    before_value: str | list[str] | int | None
    after_value: str | list[str] | int | None
    locator: Text
    scope: Literal["fragment", "field"]

    def change(self) -> ErrataChange:
        """Scope describes the supplied evidence, never an inferred printed field."""
        return ErrataChange.model_construct(**self.model_dump(exclude={"scope"}))


class Notice(RecordData):
    source: Evidence
    region: Region
    announced_on: Date | None
    effective_on: Date | None
    date_raw: str | None
    reason: str | None
    exchange_offered: bool | None
    changes: tuple[Change, ...]
    printings: tuple[ErrataPrinting, ...]
    blocks: tuple[Block, ...]
    review_note: Text

    def page(self, store_id: str) -> ErrataPage:
        """Restore the existing importer input without revalidating admitted records."""
        return ErrataPage.model_construct(
            source=self.source.source(store_id),
            region=self.region,
            official_url=self.source.url,
            announced_on=self.announced_on,
            effective_on=self.effective_on,
            date_raw=self.date_raw,
            reason=self.reason,
            exchange_offered=self.exchange_offered,
            changes=tuple(change.change() for change in self.changes),
            printings=self.printings,
        )


class Notices(RecordData):
    format: Literal[1]
    notices: tuple[Notice, ...]


class File(RecordData):
    path: Literal["jp.json", "en.json"]
    sha256: Hash
    bytes: int


class Index(RecordData):
    format: Literal[1]
    files: tuple[File, ...]


def configuration(root: Path) -> dict[str, JsonValue]:
    """Record exact uncommitted input bytes without performing admission checks."""
    directory = root / "errata"
    if not directory.exists():
        return {}
    return {
        f"{region}.json": digest((directory / f"{region}.json").read_bytes())
        for region in ("jp", "en")
    }


def read(root: Path, *, store_id: str) -> tuple[ErrataPage, ...]:
    """An absent supplemental directory preserves synthetic/minimal recipe support."""
    directory = root / "errata"
    if not directory.exists():
        return ()
    return tuple(
        notice.page(store_id)
        for region in ("jp", "en")
        for notice in decode(
            Notices, (directory / f"{region}.json").read_bytes()
        ).notices
    )


def _check_notice(notice: Notice, region: str) -> None:
    page = ErrataPage.model_validate(notice.page("admission").model_dump())
    if notice.region != region:
        raise ValueError("Errata file contains another region")
    listed = {p.card_no for p in page.printings}
    for change in page.changes:
        if change.card_no not in listed:
            raise ValueError("Errata change has no listed printing")
        validate_public(
            "ErrataChange",
            [change.face_id, change.field, change.before_value, change.after_value, []],
        )


def admit(root: Path) -> list[Notice]:
    """Admit the exact reviewed closure before returning portable notice records."""
    directory = root / "errata"
    index_path = directory / "index.json"
    if directory.is_symlink() or index_path.is_symlink():
        raise ValueError("Errata input cannot be a symlink")
    index_raw = index_path.read_bytes()
    parse(index_raw)
    index = Index.model_validate_json(index_raw)
    if tuple(pin.path for pin in index.files) != ("jp.json", "en.json"):
        raise ValueError("Errata index requires both regional files")
    if directory.is_symlink() or {p.name for p in directory.iterdir()} != {
        "jp.json",
        "en.json",
        "index.json",
    }:
        raise ValueError("Errata input has unlisted members")
    notices: list[Notice] = []
    for pin in index.files:
        path = directory / pin.path
        if path.is_symlink():
            raise ValueError("Errata input cannot be a symlink")
        raw = path.read_bytes()
        parse(raw)
        if len(raw) != pin.bytes or digest(raw) != pin.sha256:
            raise ValueError("Errata file hash/size mismatch")
        document = Notices.model_validate_json(raw)
        for notice in document.notices:
            _check_notice(notice, pin.path.removesuffix(".json"))
            notices.append(notice)
    return notices


def validate(root: Path, *, previous: Path | None = None) -> dict[str, int]:
    """Admit schema, exact regional identities and immutable prior source observations."""
    directory = root / "errata"
    if not directory.exists():
        if previous is not None and (previous / "errata").exists():
            raise ValueError("Reviewed errata history cannot be removed")
        return {"notices": 0, "printings": 0, "changes": 0}
    try:
        pages = admit(root)
    except ValidationError:
        raise ValueError("Invalid reviewed errata input") from None
    versions = [page.source.id for page in pages]
    if len(versions) != len(set(versions)):
        raise ValueError("Duplicate reviewed errata source observation")
    if previous is not None:
        current = {page.source.id: page for page in pages}
        for page in () if not (previous / "errata").exists() else admit(previous):
            candidate = current.get(page.source.id)
            if candidate is None or candidate.model_dump(
                exclude={"changes", "review_note"}
            ) != page.model_dump(exclude={"changes", "review_note"}):
                raise ValueError("Reviewed errata source history changed")
    return {
        "notices": len(pages),
        "printings": len(
            {(p.region, item.card_no) for p in pages for item in p.printings}
        ),
        "changes": sum(len(p.changes) for p in pages),
    }
