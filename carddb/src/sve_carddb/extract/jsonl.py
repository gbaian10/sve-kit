"""Write every stored card page as one JSON line per card."""

import os
import tempfile
from dataclasses import dataclass, field
from pathlib import Path
from typing import TYPE_CHECKING, Protocol

import orjson

from sve_carddb.crawl import card_numbers
from sve_carddb.extract.official_jp import extract_card
from sve_carddb.fetch.validate import ValidationError
from sve_carddb.fetch.writer import LocalState
from sve_carddb.html import MissingElementError
from sve_carddb.sources import official_jp as jp

if TYPE_CHECKING:
    from sve_carddb.manifest import Manifest


class RawReader(Protocol):
    """The small read-only boundary shared by latest and sealed sources."""

    def local_state(self, url: str) -> LocalState:
        """Report whether a source can be read."""
        ...

    def read(self, url: str) -> bytes:
        """Return the source bytes."""
        ...


@dataclass
class ExtractReport:
    """What happened to each card number of the validated lists."""

    written: int = 0
    missing: list[str] = field(default_factory=list)
    failed: dict[str, str] = field(default_factory=dict)


def extract_cards(manifest: Manifest, writer: RawReader, dest: Path) -> ExtractReport:
    """Transcribe every trusted card page of the current lists into `dest`.

    `dest` is replaced atomically, so a failed run never leaves half a file.
    """
    report = ExtractReport()
    dest.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp_name = tempfile.mkstemp(dir=dest.parent, prefix=".tmp-", suffix=".jsonl")
    tmp = Path(tmp_name)
    try:
        with os.fdopen(fd, "wb") as out:
            for number in card_numbers(manifest):
                url = jp.card_url(number)
                if writer.local_state(url) is not LocalState.TRUSTED:
                    report.missing.append(number)
                    continue
                try:
                    record = extract_card(writer.read(url), number=number)
                except (ValidationError, MissingElementError) as exc:
                    report.failed[number] = str(exc)
                    continue
                out.write(orjson.dumps(record, option=orjson.OPT_APPEND_NEWLINE))
                report.written += 1
        tmp.chmod(0o644)  # mkstemp creates 0600; match the other data files
        tmp.replace(dest)
    except BaseException:
        tmp.unlink()
        raise
    return report
