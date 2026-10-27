"""Reviewed static references; provisional entries await a contract-level decision."""

from typing import Any

_retained: Any = None
_ = (
    _retained.FrozenErrataNotices,  # Provisional: archived errata adapter; integration is not proven obsolete.
    _retained.FrozenOfficialExtras,  # Provisional: archived QA adapter; integration is not proven obsolete.
    _retained.Matcher,  # Provisional: effect-classifications.md still specifies this matching contract.
    _retained.QACrawler,  # Provisional: protected QA refresh adapter; no replacement is established.
    _retained.analyze_imports,  # Provisional: author import-closure contract still has dedicated tests.
    _retained.associate_blocks,  # Provisional: errata block association has no established replacement.
    _retained.check_source_compatibility,  # Provisional: synthetic source seam also validates production structure.
    _retained.correction_exclusions,  # Provisional: source_corrections/README.md specifies output closure.
    _retained.correction_references,  # Provisional: source_corrections/README.md specifies reference projection.
    _retained.import_glossary,  # Provisional: documented standalone atomic import; composer uses populate.
    _retained.import_links,  # Provisional: documented standalone atomic import; composer uses populate.
    _retained.load_sources,  # Provisional: construction source selection file reader has no replacement.
    _retained.lowering,  # Provisional: aggregate lowering skeleton is separate from runtime execution.
    _retained.pack,  # Provisional: definition interning contract has no established replacement.
    _retained.parse_products,  # Provisional: raw product-page boundary remains documented in products/README.md.
    _retained.parse_symbol,  # Provisional: catalog/README.md specifies this literal symbol parser.
    _retained.resolve_alias,  # Provisional: catalog/README.md specifies canonical-before-alias resolution.
    _retained.select_index_entry,  # Provisional: media index negotiation remains covered by shared vectors.
    _retained.source_document,  # Provisional: source-shard reader has rejection tests and no proven replacement.
    _retained.active_on,  # Provisional: restriction date semantics have no established replacement.
    _retained.assert_identical,  # Provisional: snapshot-export.md specifies exact repeat-export comparison.
    _retained.author_effect_set,  # Provisional: author-entry-shapes-1.0.md still specifies this typed reader.
    _retained.browse,  # Provisional: transitions/README.md specifies effective identity browsing.
    _retained.card_pages,  # Provisional: archived supplemental parser belongs to the retained QA adapter.
    _retained.effect_body_shard,  # Provisional: author-entry-shapes-1.0.md still specifies this typed reader.
    _retained.expand_sources,  # Provisional: documented source-admission seam; current uses expand_contexts.
    _retained.from_card,  # Provisional: preview/README.md specifies source-derived card evidence.
    _retained.import_into,  # Provisional: documented standalone observation transaction boundary.
    _retained.parameter_reference,  # Provisional: typed author leaf decoder; removal needs contract review.
    _retained.parameter_value,  # Provisional: typed author leaf decoder; removal needs contract review.
    _retained.parameter_values,  # Provisional: typed author leaf decoder; removal needs contract review.
    _retained.require,  # Provisional: executable ruling closure guard; production consumer is pending.
    _retained.require_usable,  # Provisional: build/README.md specifies importer/validator readiness checks.
    _retained.resolve_int_id,  # Provisional: transitions/README.md specifies original printing repair hints.
    _retained.outcomes,  # Provisional: manifest request-history query remains used by ingest tests.
    _retained.unbound,  # Pytest injects fixture imports in rulings/test_mapping.py and test_rulings.py.
    _retained.plain_cli_output,  # Pytest autouse fixture in tests/conftest.py; no named consumer is needed.
    _retained.current_lowering_inputs,  # Python subprocess in sim/engine/src/ir/tests/lowering.rs imports this helper.
)
