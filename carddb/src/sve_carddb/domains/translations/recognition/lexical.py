"""Source grammar receives the authoritative partition and Unicode trace."""

import re
from dataclasses import dataclass
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from sve_carddb.contracts.four_layer import Role, Span
    from sve_carddb.domains.translations.recognition.provenance import Unit

DIGITS = re.compile(r"[0-9０-９]+")
QUOTED = re.compile(r"『[^』]*』")


@dataclass(frozen=True)
class Part:
    role: Role
    segments: tuple[Span, ...]
    normalized: str
    units: tuple[Unit, ...]
