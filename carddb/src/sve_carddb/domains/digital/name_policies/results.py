"""Database projection result shared by name-policy entry points."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from collections.abc import Mapping

    from pydantic import JsonValue

    from sve_carddb.core.provenance import InputRecord
    from sve_carddb.domains.translations.four_layer_labels import ReferenceName
    from sve_carddb.domains.translations.names.bindings import DisplayBinding


@dataclass(frozen=True)
class Result:
    record: InputRecord
    bindings: tuple[DisplayBinding, ...]
    report: dict[str, JsonValue]
    reference_names: Mapping[str, ReferenceName]
    card_name_targets: Mapping[str, tuple[str, ...]]
    card_name_fragments: frozenset[str]
