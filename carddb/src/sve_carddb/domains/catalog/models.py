"""Memory catalog projections for checked adoptions and explicit synthetic staging."""

from typing import Literal

from sve_carddb.core.models import RecordData, Text
from sve_carddb.core.regions import Region
from sve_carddb.domains.catalog.symbols import Symbol
from sve_carddb.domains.products.models import Code, Lang, Language, LocalizedText


class Term(RecordData):
    kind: Code
    code: Code
    label: LocalizedText
    active: bool = True


class Alias(RecordData):
    kind: Code
    code: Text
    lang: Lang
    text: Text
    normalized: Text
    decision_id: Text | None = None


class NameBinding(RecordData):
    face_id: Text
    region: Region
    official_name: Text
    # An explicit primary is only accepted where no observation can derive one.
    role: Literal["primary", "collab", "treated_as"]
    # Current-format adoptions have no decision envelope.
    decision_id: Text | None = None


class Catalog(RecordData):
    languages: tuple[Language, ...]
    terms: tuple[Term, ...]
    aliases: tuple[Alias, ...]
    symbols: tuple[Symbol, ...]
    normalizer_version: Text
    names: tuple[NameBinding, ...] = ()
