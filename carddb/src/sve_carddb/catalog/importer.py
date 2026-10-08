"""Transaction-local catalog projection with explicit build configuration pins."""

from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build_db import Json
from sve_carddb.build_db.rows import insert_exact
from sve_carddb.catalog.languages import register_languages
from sve_carddb.catalog.models import Catalog
from sve_carddb.catalog.rules_names import populate_rules_names, register_name
from sve_carddb.core.json import canonical, digest, object_value, parse
from sve_carddb.text_observations.intern import TextInterner

if TYPE_CHECKING:
    from sve_carddb.build_db import Database
    from sve_carddb.core.provenance import BuildContext
    from sve_carddb.products.models import LocalizedText


def catalog_configuration(
    catalog: Catalog, published: tuple[LocalizedText, ...] = ()
) -> dict[str, JsonValue]:
    """Pin the complete typed catalog, including caller-supplied normalization bytes."""
    return {
        "catalog": catalog.model_dump(mode="json"),
        "catalog_published_text_hash": digest(
            canonical(
                list[JsonValue](
                    sorted(
                        {
                            canonical(text.model_dump(mode="json")).decode()
                            for text in published
                        }
                    )
                )
            )
        ),
    }


def _adopted(db: Database, decision_id: str) -> None:
    """Keep caller-only staging separate from the authored adoption loader."""
    decisions = {row.values["id"]: row.values for row in db.rows("decision")}
    sources = {row.values["id"]: row.values for row in db.rows("source_record")}
    links = [
        row.values
        for row in db.rows("decision_source")
        if row.values["decision_id"] == decision_id
    ]
    if (
        decision_id not in decisions
        or decisions[decision_id]["state"] != "confirmed"
        or not links
    ):
        raise ValueError("Catalog adoption requires confirmed sourced decision")
    if not any(sources[link["source_id"]]["kind"] == "authored" for link in links):
        raise ValueError("Catalog adoption requires authored evidence")
    # A sourced confirmed decision alone cannot bind this input to its reviewed bytes.
    raise ValueError(
        "Catalog staging cannot verify category, exact members or freshness; "
        "decision-backed catalog staging is not supported"
    )


def populate_catalog(
    db: Database,
    catalog: Catalog,
    *,
    build: BuildContext,
    published: tuple[LocalizedText, ...],
) -> None:
    """Project synthetic staging inputs without admitting decision-backed values."""
    catalog = Catalog.model_validate_json(catalog.model_dump_json())
    config = object_value(parse(build.configuration.encode()))
    if any(
        config.get(key) != value
        for key, value in catalog_configuration(catalog, published).items()
    ):
        raise ValueError("Build configuration does not pin the catalog")
    register_languages(db, catalog.languages)
    texts = TextInterner(db, published=published)
    for term in catalog.terms:
        insert_exact(
            db,
            "vocabulary",
            {
                "kind": term.kind,
                "code": term.code,
                "label_unit_id": texts.intern(term.label),
                "active": term.active,
            },
            ("kind", "code"),
        )
    for alias in catalog.aliases:
        if alias.decision_id is not None:
            _adopted(db, alias.decision_id)
        insert_exact(
            db,
            "search_alias",
            {
                "kind": alias.kind,
                "code": alias.code,
                "lang": alias.lang,
                "text": alias.text,
                "normalized": alias.normalized,
                "normalizer_version": catalog.normalizer_version,
                "decision_id": alias.decision_id,
            },
            ("kind", "code", "lang", "text"),
        )
    languages = texts.languages
    for symbol in catalog.symbols:
        _adopted(db, symbol.decision_id)
        if any(item.lang not in languages for item in symbol.spellings) or any(
            item.lang not in languages for item in symbol.localizations
        ):
            raise ValueError("Symbol language is not registered")
        data = symbol.model_dump(mode="json")
        insert_exact(
            db,
            "text_symbol",
            {
                "id": symbol.id,
                "code": symbol.code,
                "parameter_schema": Json(symbol.parameter_schema),
                "keyword_id": symbol.keyword_id,
                "spellings": Json(data["spellings"]),
                "localizations": Json(data["localizations"]),
                "decision_id": symbol.decision_id,
            },
            ("id",),
        )
    populate_rules_names(db)
    for binding in catalog.names:
        _adopted(db, binding.decision_id)
        register_name(db, binding)
    db.verify_alias_targets()


def resolve_alias(
    db: Database,
    kind: str,
    lang: str,
    text: str,
    *,
    normalized: str,
    normalizer_version: str,
) -> tuple[str, ...]:
    """Prefer exact canonical codes; return every matching alias rather than choosing."""
    canonical_codes = db.alias_target_codes(kind)
    if text in canonical_codes:
        return (text,)
    aliases = [
        row.values
        for row in db.rows("search_alias")
        if row.values["kind"] == kind and row.values["lang"] == lang
    ]
    if any(row["normalizer_version"] != normalizer_version for row in aliases):
        raise ValueError("Unsupported alias normalizer version")
    return tuple(
        sorted({str(row["code"]) for row in aliases if row["normalized"] == normalized})
    )
