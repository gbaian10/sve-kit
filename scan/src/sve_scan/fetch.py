"""Polite downloads with a per-file provenance record (AGENTS.md crawling rules): at least 2 seconds between
any two requests (API calls included), and for every file its source URL, fetch time, ETag and SHA-256.

A folder's manifest.json keeps the record of each file; a file that is already there is not fetched again and
keeps its earlier record, so a rerun never erases what the first download saw.
"""

import hashlib
import json
import time
import urllib.request
from collections.abc import Callable
from datetime import UTC, datetime
from email.message import Message
from pathlib import Path
from typing import Any

MIN_GAP = 2.0
UA = {"User-Agent": "sve-kit-scan/0 (CC0 texture fetch for synthetic training data)"}

Record = dict[str, Any]


def _open(url: str) -> tuple[bytes, Message]:
    with urllib.request.urlopen(urllib.request.Request(url, headers=UA), timeout=120) as r:
        return r.read(), r.headers


class Polite:
    def __init__(
        self,
        folder: Path,
        clock: Callable[[], float] = time.monotonic,
        sleep: Callable[[float], None] = time.sleep,
        opener: Callable[[str], tuple[bytes, Message]] = _open,
    ) -> None:
        self.folder = folder
        folder.mkdir(parents=True, exist_ok=True)
        self.clock, self.sleep, self.opener = clock, sleep, opener
        self.last: float | None = None
        path = folder / "manifest.json"
        rows = json.loads(path.read_text()) if path.exists() else []
        self.records: dict[str, Record] = {r["file"]: r for r in rows}

    def _get(self, url: str) -> tuple[bytes, Message]:
        if self.last is not None:
            wait = MIN_GAP - (self.clock() - self.last)
            if wait > 0:
                self.sleep(wait)
        try:
            return self.opener(url)
        finally:
            self.last = self.clock()

    def json(self, url: str) -> Any:
        return json.loads(self._get(url)[0])

    def complete(self, name: str) -> bool:
        """The file is here and its record names where it came from; only then is it never fetched again."""
        r = self.records.get(name, {})
        return (self.folder / name).exists() and bool(r.get("url")) and "sha256" in r and "bytes" in r

    def download(self, url: str, name: str, **meta: Any) -> Record:
        """Fetch url into folder/name unless it is complete already; return the file's record.

        The record reaches manifest.json before the file takes its name, so a run that dies later never
        leaves a file whose fetch time and ETag are forgotten.
        """
        dst = self.folder / name
        if self.complete(name):
            self.records[name].update(meta)
            self.save()
            return self.records[name]
        if dst.exists():  # here but never (fully) recorded: describe the file, admit what is unknown
            body, etag, fetched_at = dst.read_bytes(), None, None
        else:
            body, headers = self._get(url)
            etag, fetched_at = headers.get("ETag"), datetime.now(UTC).isoformat(timespec="seconds")
            tmp = dst.with_name(dst.name + ".part")
            tmp.write_bytes(body)
        record = {
            "file": name,
            **meta,
            "url": url,
            "fetched_at": fetched_at,
            "etag": etag,
            "sha256": hashlib.sha256(body).hexdigest(),
            "bytes": len(body),
        }
        self.records[name] = record
        self.save()
        if not dst.exists():
            tmp.replace(dst)
        return record

    def save(self) -> None:
        rows = [self.records[k] for k in sorted(self.records)]
        tmp = self.folder / "manifest.json.part"
        tmp.write_text(json.dumps(rows, ensure_ascii=False, indent=1) + "\n")
        tmp.replace(self.folder / "manifest.json")
