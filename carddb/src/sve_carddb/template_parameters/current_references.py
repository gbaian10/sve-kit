"""Current glossary references without a historical template interpreter."""

from typing import TYPE_CHECKING

from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.template_parameters.references import References
from sve_carddb.translations.current_models import TermRecord
from sve_carddb.translations.sources import excerpt

if TYPE_CHECKING:
    from sve_carddb.template_parameters.references import Evidence
    from sve_carddb.translations.loader import Snapshot


def adopted(snapshot: Snapshot, sources: Evidence) -> References:
    """Reuse the full glossary closure and frozen source validator before exact lookup."""
    result = References()
    records = snapshot.current_records()
    for record in records:
        if not isinstance(record, TermRecord):
            continue
        data = record.data
        if data.source_ref is None:
            raw = data.authored_source_ja
        else:
            lang, text, _ = sources.text(data.source_ref)
            if lang != "ja":
                raise ValueError(
                    "Parameter concepts require exact Japanese source names"
                )
            raw = excerpt(text, data.source_span)
        if not isinstance(raw, str) or not raw:
            raise ValueError("Parameter concept source must be nonempty exact text")
        result.terms.setdefault(raw, []).append((data.id, data.category))
        if data.category == "card_name":
            result.card_names.setdefault(raw, []).append(data.id)
    result.pins["exact_concepts_hash"] = digest(
        canonical(
            {
                digest(raw.encode()): [list(item) for item in values]
                for raw, values in sorted(result.terms.items())
            }
        )
    )
    return result
