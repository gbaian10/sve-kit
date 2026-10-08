"""Database projection result shared by name-policy entry points."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.core.provenance import InputRecord
    from sve_carddb.domains.translations.names.bindings import DisplayBinding


@dataclass(frozen=True)
class Result:
    record: InputRecord
    bindings: tuple[DisplayBinding, ...]
    report: dict[str, JsonValue]
