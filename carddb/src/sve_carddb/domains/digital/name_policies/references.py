"""Official reference names use validated terms, without borrowing owner exceptions."""

import re
from typing import TYPE_CHECKING

from sve_carddb.domains.digital.name_policies.evaluate import exact_name_result
from sve_carddb.domains.translations.four_layer_labels import ReferenceName
from sve_carddb.domains.translations.glossary.records import TermRecord

if TYPE_CHECKING:
    from sve_carddb.domains.digital.name_policies.catalogue import Catalogue
    from sve_carddb.domains.translations.names.resolve import Names


def reference_names(
    replay: Names, frozen: Catalogue, *, low_confidence: bool
) -> dict[str, ReferenceName]:
    """Only a term's whole frozen name field can qualify as a complete name."""
    originals = dict(replay.originals)
    result = {}
    for record in replay.snapshot.current_records():
        if not isinstance(record, TermRecord) or record.data.category != "card_name":
            continue
        term = record.data
        ref = term.source_ref
        if ref is None or term.source_span is not None:
            continue
        pattern = {
            "translation-jp-v1": r"/faces/[0-9]+/name",
            "translation-sv1-v1": r"/data/cards/[0-9]+/card_name",
        }.get(ref.parser)
        if pattern is None or re.fullmatch(pattern, ref.locator) is None:
            continue
        outcome = exact_name_result(originals[term.id], frozen)
        if outcome.status == "eligible":
            assert outcome.text is not None
            result[term.id] = ReferenceName(outcome.text, low_confidence)
    return result
