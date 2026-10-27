"""Produce a public logical view, independent of tuple partitioning or publication."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.contracts.snapshot import definition, tables
from sve_carddb.core.json import array, canonical, object_value, string
from sve_carddb.domains.routes.defaults import select_defaults
from sve_carddb.export.project.annotations import annotations
from sve_carddb.export.project.closure import (
    prune,
    select_regions,
    validate_closure,
    validate_identities,
)
from sve_carddb.export.project.display import DisplayText, display_text
from sve_carddb.export.project.evidence import Decisions, DisplayCheck
from sve_carddb.export.project.metadata import Settings, configuration, summaries
from sve_carddb.export.project.observations import corrections, observations
from sve_carddb.export.project.records import (
    ancillary_records,
    art_records,
    digital_records,
    initial,
    printing_records,
    ruling_records,
    text_records,
)
from sve_carddb.export.project.regions import (
    Dates,
    effective_support,
    inclusions,
    region_views,
    support,
)
from sve_carddb.export.project.shape import tuple_value
from sve_carddb.export.project.source import Record, Source, json_list
from sve_carddb.export.project.translations import Texts, keywords, translations
from sve_carddb.export.semantics import ordered_rows, row_key

if TYPE_CHECKING:
    from sve_carddb.build import Database

__all__ = [
    "Decisions",
    "DisplayCheck",
    "DisplayText",
    "Projection",
    "Settings",
    "display_text",
    "effective_support",
    "project",
]


@dataclass(frozen=True)
class Projection:
    tables: dict[str, list[Record]]
    config: Record
    metadata: Record


def _sort(view: dict[str, list[Record]]) -> None:
    for table, rows in view.items():
        keys = [string(field) for field in array(definition(table)["x-primary-key"])]
        rows.sort(key=lambda row: tuple(row_key(row[field]) for field in keys))
        ordered_rows(rows, keys)
        for row in rows:
            _nested_sort(row)


def _nested_sort(row: Record) -> None:
    for name, keys in (
        ("translations", ["field", "ordinal", "target_lang"]),
        ("wording_translations", ["lang"]),
        ("sections", ["ordinal"]),
        ("current", ["region"]),
        ("regions", ["region"]),
    ):
        if name in row and all(isinstance(item, dict) for item in array(row[name])):
            values = [object_value(raw) for raw in array(row[name])]
            values.sort(
                key=lambda value: tuple(
                    (0, "") if value[key] is None else (1, value[key]) for key in keys
                )
            )
            ordered_rows(values, keys)
            row[name] = json_list(values)
    for value in row.values():
        if isinstance(value, list):
            for item in value:
                if isinstance(item, dict):
                    _nested_sort(item)
        elif isinstance(value, dict):
            _nested_sort(value)


def _public_keys(view: dict[str, list[Record]]) -> None:
    # Build IDs finish evidence joins before public relations become value sets.
    for table in ("card_related", "digital_link"):
        for row in view[table]:
            del row["id"]
        view[table] = list({canonical(row): row for row in view[table]}.values())


def _validate(view: dict[str, list[Record]]) -> None:
    if set(view) != set(tables()):
        raise ValueError("Public collection whitelist mismatch")
    for table, rows in view.items():
        for row in rows:
            tuple_value(table, row)
    validate_closure(view)
    validate_identities(view)


def _related(
    view: dict[str, list[Record]], regions: tuple[str, ...], decisions: Decisions
) -> None:
    for row in view["card_related"]:
        row["applicable_regions"] = None
        if row["relation"] == "same_rules_reskin":
            row["applicable_regions"] = json_list(
                sorted(
                    set(decisions.related_regions.get(string(row["id"]), ()))
                    & set(regions)
                )
            )
    view["card_related"] = [
        row
        for row in view["card_related"]
        if row["relation"] != "same_rules_reskin" or row["applicable_regions"]
    ]


def _rulings(view: dict[str, list[Record]], decisions: Decisions) -> None:
    for row in view["ruling_revision"]:
        if string(row["id"]) not in decisions.active_scopes:
            raise ValueError("Ruling active scopes require verified build projection")
        row["active_scopes"] = list(decisions.active_scopes[string(row["id"])])
        if not row["active_scopes"] or row["strength"] == "undecided":
            row["hints"] = []


def project(
    db: Database,
    *,
    regions: tuple[str, ...],
    settings: Settings,
    decisions: Decisions = Decisions(),
    publication_printings: frozenset[str] | None = None,
) -> Projection:
    """Project verified offline build rows; no filesystem writes or live source access."""
    if (
        not regions
        or tuple(sorted(set(regions))) != regions
        or not set(regions) <= {"jp", "en"}
    ):
        raise ValueError("Explicit sorted nonempty regions required")
    if type(decisions.digital_assets) is not bool:
        raise ValueError("Digital asset projection flag must be boolean")
    source = Source(db)
    view = initial(source)
    if publication_printings is not None:
        if not publication_printings <= {row["id"] for row in view["printing"]}:
            raise ValueError("Publication printing is absent from verified build")
        # Diagnostic staging retains parents that have not been adopted for publication.
        view["printing"] = [
            row for row in view["printing"] if row["id"] in publication_printings
        ]
    select_regions(view, regions)
    text_records(source, view)
    printing_records(source, view)
    ancillary_records(source, view)
    art_records(source, view)
    digital_records(source, view)
    if decisions.digital_links is not None:
        view["digital_link"] = [
            row for row in view["digital_link"] if row["id"] in decisions.digital_links
        ]
    if not decisions.digital_assets:
        for table in (
            "digital_art",
            "digital_art_link",
            "voice",
            "card_voice",
        ):
            view[table] = []
    ruling_records(source, view)
    _rulings(view, decisions)
    _related(view, regions, decisions)
    texts = Texts(view["text_unit"])
    keywords(source, view, texts)
    translations(source, view, texts, decisions)
    observations(source, view, decisions)
    corrections(source, view)
    dates = Dates(source, view)
    inclusions(source, view, dates)
    public_prints = {row["id"] for row in view["printing"]}
    defaults = {
        (item.card_id, item.region): item
        for item in select_defaults(db)
        if item.printing_id in public_prints
    }
    region_views(source, view, dates, defaults)
    support(source, view, decisions)
    annotations(source, view, decisions)
    prune(view)
    _public_keys(view)
    _sort(view)
    _validate(view)
    config = configuration(source, settings)
    metadata = summaries(source, view, regions, decisions)
    if (
        not decisions.digital_assets
        and not view["digital_link"]
        and not view["digital_link_coverage"]
    ):
        config["digital_endpoints"] = []
    return Projection(view, config, metadata)
