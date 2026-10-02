"""Memory-only catalog and exact bindings derived from checked effective adoptions."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.catalog import adoption_validation as validate
from sve_carddb.catalog.adoption_models import (
    AliasRecord,
    LanguageRecord,
    NameRecord,
    SymbolRecord,
    VocabularyRecord,
)
from sve_carddb.catalog.models import Catalog
from sve_carddb.text_observations.vocabulary import Binding, Vocabulary

if TYPE_CHECKING:
    from sve_carddb.catalog.adoption_models import Record, ReviewContext
    from sve_carddb.catalog.adoption_sources import AdoptionSources
    from sve_carddb.catalog.models import NameBinding
    from sve_carddb.text_observations.plan import TextPlan


@dataclass(frozen=True)
class CatalogProjection:
    catalog: Catalog
    vocabulary: Vocabulary


def project_catalog(  # ruff: ignore[complex-structure] -- typed effective kinds share one derived catalog with no alternate authority
    effective: tuple[tuple[Record, str], ...],
    reviews: dict[str, ReviewContext],
    sources: AdoptionSources,
    plan: TextPlan | None,
) -> CatalogProjection:
    """Consume only the importer's validated history, dependencies and freshness."""
    registered = {
        record.data.subject.code
        for record, _ in effective
        if isinstance(record, LanguageRecord) and record.data.value is not None
    }
    languages = []
    terms = []
    aliases = []
    symbols = []
    names: list[NameBinding] = []
    bindings: list[Binding] = []
    for record, decision in effective:
        review = reviews[record.record_key]
        if isinstance(record, LanguageRecord):
            if (language := validate.language(record, registered)) is not None:
                languages.append(language)
        elif isinstance(record, VocabularyRecord):
            if (term := validate.term(record, review, sources)) is not None:
                terms.append(term)
                value = record.data.value
                assert value is not None
                if term.active:
                    bindings.extend(
                        Binding(
                            region=mapping.region,
                            kind=term.kind,
                            raw=mapping.raw,
                            code=term.code,
                            special_kinds=mapping.special_kinds,
                        )
                        for mapping in value.raw_mappings
                    )
        elif isinstance(record, AliasRecord):
            if (alias := validate.alias(record, review, sources)) is not None:
                aliases.append(alias.model_copy(update={"decision_id": decision}))
        elif isinstance(record, SymbolRecord):
            if (
                symbol := validate.symbol(record, review, sources, decision)
            ) is not None:
                symbols.append(symbol)
        elif isinstance(record, NameRecord):
            names.extend(validate.names(record, review, sources, plan, decision))
    catalog = Catalog(
        languages=tuple(languages),
        terms=tuple(terms),
        aliases=tuple(aliases),
        symbols=tuple(symbols),
        names=tuple(names),
        normalizer_version="nfkc-casefold-v1",
    )
    vocabulary = Vocabulary(bindings=tuple(bindings), terms=catalog.terms)
    vocabulary.verify()
    return CatalogProjection(catalog, vocabulary)
