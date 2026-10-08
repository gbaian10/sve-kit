"""Produce a public logical view, independent of tuple partitioning or publication."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.core.json import array, canonical, object_value, parse, string
from sve_carddb.routes.defaults import select_defaults
from sve_carddb.snapshot.contract import definition, tables, validate
from sve_carddb.snapshot.project.closure import (
    prune,
    select_regions,
    validate_closure,
    validate_identities,
)
from sve_carddb.snapshot.project.display import DisplayText, display_text
from sve_carddb.snapshot.project.evidence import Decisions, DisplayBinding, DisplayCheck
from sve_carddb.snapshot.project.metadata import Settings, configuration, summaries
from sve_carddb.snapshot.project.observations import corrections, observations
from sve_carddb.snapshot.project.records import (
    ancillary_records,
    art_records,
    digital_records,
    initial,
    printing_records,
    ruling_records,
    text_records,
)
from sve_carddb.snapshot.project.regions import (
    Dates,
    effective_support,
    inclusions,
    region_views,
    support,
)
from sve_carddb.snapshot.project.shape import source_tuple
from sve_carddb.snapshot.project.source import Record, Source, json_list
from sve_carddb.snapshot.project.translations import Texts, keywords, translations
from sve_carddb.snapshot.semantics import ordered_rows, validate_view

if TYPE_CHECKING:
    from sve_carddb.build_db import Database

__all__ = [
    "Decisions",
    "DisplayBinding",
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
        rows.sort(
            key=lambda row: tuple(
                (0, "") if row[field] is None else (1, row[field]) for field in keys
            )
        )
        ordered_rows(rows, keys)
        for row in rows:
            _nested_sort(row)


def _nested_sort(row: Record) -> None:
    for name, keys in (
        ("translations", ["field", "ordinal", "target_lang"]),
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


def _validate(view: dict[str, list[Record]]) -> None:
    if set(view) != set(tables()):
        raise ValueError("Public collection whitelist mismatch")
    for table, rows in view.items():
        for row in rows:
            candidate = object_value(parse(canonical(row)))
            source_tuple(table, candidate)
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


def _images(source: Source, view: dict[str, list[Record]]) -> None:
    assets = source.index("image_asset", "id,mime,publication_state,availability")
    for asset in view["image_asset"]:
        mime = assets[string(asset["id"])]["mime"]
        asset["format"] = None if mime is None else string(mime).removeprefix("image/")
    for row in view["printing_image"]:
        state = assets[string(row["image_id"])]
        row["publication_state"] = state["publication_state"]
        row["availability"] = state["availability"]
    allowed = {
        row["image_id"]
        for row in view["printing_image"]
        if row["publication_state"] == "approved" and row["availability"] == "available"
    }
    view["image_variant"] = [
        row for row in view["image_variant"] if row["image_id"] in allowed
    ]


def project(
    db: Database,
    *,
    regions: tuple[str, ...],
    as_of: str,
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
    if type(decisions.private_digital) is not bool:
        raise ValueError("Private digital projection flag must be boolean")
    validate("Date", as_of)
    db.verify()
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
    if decisions.private_digital:
        for table in (
            "digital_card",
            "digital_art",
            "digital_link",
            "digital_art_link",
            "digital_link_coverage",
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
        for item in select_defaults(db, general_evidence=decisions.general_evidence)
        if item.printing_id in public_prints
    }
    region_views(source, view, as_of, dates, defaults)
    support(source, view, decisions)
    _images(source, view)
    prune(view)
    _sort(view)
    _validate(view)
    config = configuration(source, settings)
    metadata = summaries(source, view, regions, decisions)
    if decisions.private_digital:
        config["digital_endpoints"] = []
    validate_view(view, metadata, [])
    return Projection(view, config, metadata)
