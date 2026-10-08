"""Exact UTF-8 interning against this build and a supplied published-text union."""

import hashlib
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from sve_carddb.build import Database, Value
    from sve_carddb.products.models import LocalizedText


def text_values(text: LocalizedText) -> dict[str, Value]:
    """Compute the fixed namespace and full exact-byte digest without normalization."""
    checksum = hashlib.sha256(text.text.encode("utf-8")).hexdigest()
    return {
        "id": f"t:{text.lang}:{checksum[:16]}",
        "lang": text.lang,
        "text": text.text,
        "content_hash": "sha256:" + checksum,
    }


class TextInterner:
    def __init__(
        self, db: Database, *, published: tuple[LocalizedText, ...] = ()
    ) -> None:
        self.db = db
        self.languages = {row.values["code"] for row in db.rows("language")}
        self.ids = {row.values["id"]: dict(row.values) for row in db.rows("text_unit")}
        self.hashes = {
            (row["lang"], row["content_hash"]): row for row in self.ids.values()
        }
        self.persisted = set(self.ids)
        for text in published:
            self._remember(text_values(text))

    def _remember(self, values: dict[str, Value]) -> None:
        for previous in (
            self.ids.get(values["id"]),
            self.hashes.get((values["lang"], values["content_hash"])),
        ):
            if previous is not None and previous != values:
                raise ValueError(
                    "Text unit hash/ID collision or inconsistent exact bytes"
                )
        self.ids[values["id"]] = values
        self.hashes[values["lang"], values["content_hash"]] = values

    def intern(self, text: LocalizedText) -> str:
        """Reuse only exact bytes; collisions never lengthen or replace published IDs."""
        if text.lang not in self.languages:
            raise ValueError("Text language is not registered")
        values = text_values(text)
        self._remember(values)
        identifier = values["id"]
        assert isinstance(identifier, str)
        if identifier not in self.persisted:
            self.db.insert("text_unit", values)
            self.persisted.add(identifier)
        return identifier
