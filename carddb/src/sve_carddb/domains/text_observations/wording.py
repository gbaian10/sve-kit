"""Printing observations and reviewed semantic bases remain separate projections."""

from collections import defaultdict
from typing import TYPE_CHECKING, Literal

from pydantic import JsonValue, model_validator

from sve_carddb.build import Json
from sve_carddb.core.json import string
from sve_carddb.core.models import RecordData, Text
from sve_carddb.core.regions import Region
from sve_carddb.domains.products.models import Date, Precision, check_date
from sve_carddb.domains.registry.records import CorrectionData, PrintingData
from sve_carddb.domains.text_observations.equivalence import (
    Classification,
    Method,
    WordingDecision,
    comparison_hash,
    equivalent,
)
from sve_carddb.domains.text_observations.models import candidate_revision_id

if TYPE_CHECKING:
    from sve_carddb.build import Database, Value
    from sve_carddb.domains.text_observations.plan import TextPlan


def _string(value: object) -> str:
    if not isinstance(value, str):
        raise TypeError("Expected validated build string")
    return value


def _region(value: object) -> Region:
    if value == "jp":
        return "jp"
    if value == "en":
        return "en"
    raise ValueError("Expected build region")


def _revision_options(revisions: set[str | None]) -> tuple[str | None, ...]:
    return (
        tuple(sorted(revisions, key=lambda value: value or ""))
        if revisions
        else (None,)
    )


class WordingCandidate(RecordData):
    printing_id: Text
    revision_id: Text | None


class WordingDisplay(RecordData):
    revision_id: Text | None
    basis: Literal["current", "latest_known_release", "candidates"]


class WordingView(RecordData):
    region: Region
    state: Literal["pending"] = "pending"
    display: WordingDisplay
    candidates: tuple[WordingCandidate, ...]
    undated_printing_ids: tuple[Text, ...]
    classification: Classification = "unresolved"
    method: Method = "none"
    reason: Text = "not_assessed"
    basis_printing_id: Text | None = None
    basis_released_on: Date | None = None

    def wire(self) -> list[JsonValue]:
        """Name selections are resolved later by the export projection."""
        return [
            self.region,
            self.state,
            [self.display.revision_id, self.display.basis],
            [[c.printing_id, c.revision_id] for c in self.candidates],
            list[JsonValue](self.undated_printing_ids),
            None,
            self.classification,
            self.method,
            self.reason,
            self.basis_printing_id,
            self.basis_released_on,
        ]


class ObservedText(RecordData):
    revision_id: Text | None
    state: Literal["available", "missing_effect", "correction_conflict"]
    source_url: Text

    def wire(self) -> list[JsonValue]:
        """Keep source observations separate from printed/current values."""
        return [self.revision_id, self.state, self.source_url]


class _ProductDate(RecordData):
    id: Text
    region: Region
    released_on: Date | None
    date_precision: Precision
    date_raw: str | None

    @model_validator(mode="after")
    def dates(self) -> _ProductDate:
        check_date(self.released_on, self.date_precision, self.date_raw)
        return self


class _InclusionDate(RecordData):
    printing_id: Text
    product_id: Text
    first_available_on: Date | None
    first_available_precision: Precision | None
    first_available_raw: str | None

    @model_validator(mode="after")
    def dates(self) -> _InclusionDate:
        check_date(
            self.first_available_on,
            self.first_available_precision,
            self.first_available_raw,
        )
        return self


def printing_dates(db: Database) -> dict[str, str | None]:
    """Prove first availability only when every inclusion has a complete day."""
    products = {
        p.id: p
        for row in db.rows("product")
        for p in (
            _ProductDate.model_validate(
                {key: row.values[key] for key in _ProductDate.model_fields}
            ),
        )
    }
    regions = {
        _string(row.values["id"]): _string(row.values["region"])
        for row in db.rows("printing")
    }
    dates: dict[str, list[str | None]] = defaultdict(list)
    for row in db.rows("printing_product"):
        inclusion = _InclusionDate.model_validate(
            {key: row.values[key] for key in _InclusionDate.model_fields}
        )
        product = products[inclusion.product_id]
        if regions[inclusion.printing_id] != product.region:
            raise ValueError("Wording date product/printing region mismatch")
        precision = inclusion.first_available_precision
        date = inclusion.first_available_on
        if precision is None:
            precision, date = product.date_precision, product.released_on
        dates[inclusion.printing_id].append(date if precision == "day" else None)
    return {
        printing: min(value for value in dates[printing] if value is not None)
        if dates[printing] and all(value is not None for value in dates[printing])
        else None
        for printing in regions
    }


def _display(
    current: str | None,
    candidates: tuple[WordingCandidate, ...],
    dates: dict[str, str | None],
    blocked: frozenset[str],
    contents: dict[str, str],
) -> WordingDisplay:
    if current is not None:
        return WordingDisplay(revision_id=current, basis="current")
    known = [
        dates[c.printing_id] for c in candidates if dates[c.printing_id] is not None
    ]
    if known:
        latest = max(value for value in known if value is not None)
        newest = [c for c in candidates if dates[c.printing_id] == latest]
        revisions = {c.revision_id for c in newest}
        if (
            None not in revisions
            and len({contents[r] for r in revisions if r is not None}) == 1
            and not any(c.printing_id in blocked for c in newest)
        ):
            return WordingDisplay(
                revision_id=min(r for r in revisions if r is not None),
                basis="latest_known_release",
            )
    return WordingDisplay(revision_id=None, basis="candidates")


def _candidate_revisions(
    db: Database, plan: TextPlan, parents: dict[tuple[str, Region], set[str]]
) -> dict[tuple[str, str], set[str | None]]:
    revisions = {
        _string(row.values["id"]): row.values for row in db.rows("face_revision")
    }
    observations: dict[tuple[str, str], set[str | None]] = defaultdict(set)
    for item in plan.candidates():
        if item.printing_id not in parents.get((item.face_id, item.region), set()):
            continue
        identifier = (
            None if item.content.effect is None else candidate_revision_id(item)
        )
        if identifier is not None:
            target = revisions.get(identifier)
            if (
                target is None
                or target["face_id"] != item.face_id
                or target["region"] != item.region
            ):
                raise ValueError(
                    "Wording candidate revision is missing or belongs to another face/region"
                )
        observations[item.printing_id, item.face_id].add(identifier)
    return observations


def _correction_blocks(plan: TextPlan, face: str, region: Region) -> frozenset[str]:
    applied = {
        item.data.id
        for item in plan.corrections or ()
        if item.status in {"applied", "already_fixed"}
    }
    blocked = set()
    for record in plan.identity.snapshot.records.values():
        data = record.data
        if (
            isinstance(data, CorrectionData)
            and data.face_id == face
            and data.id not in applied
        ):
            printing = plan.identity.snapshot.records[
                "printing:" + data.printing_id
            ].data
            if isinstance(printing, PrintingData) and printing.region == region:
                blocked.add(data.printing_id)
    return frozenset(blocked)


def wording_views(db: Database, plan: TextPlan) -> dict[str, tuple[WordingView, ...]]:
    """Keep every identity-eligible face/region, including missing-source positions."""
    dates = printing_dates(db)
    parents: dict[tuple[str, Region], set[str]] = defaultdict(set)
    for record in plan.publication_identity().included("printing"):
        p = record.data
        if isinstance(p, PrintingData):
            for mapping in p.source_face_map:
                parents[mapping.face_id, p.region].add(p.id)
    currents = {
        (_string(row.values["face_id"]), _region(row.values["region"])): _string(
            row.values["revision_id"]
        )
        for row in db.rows("face_current")
    }
    observations = _candidate_revisions(db, plan, parents)
    contents = {
        candidate_revision_id(item): item.content.fingerprint()
        for item in plan.candidates()
        if item.content.effect is not None
    }
    groups = {(g.face_id, g.region): g for g in plan.groups}
    assessments = {
        (_string(row.values["face_id"]), _string(row.values["region"])): row.values
        for row in db.rows("face_wording")
    }
    result: dict[str, list[WordingView]] = defaultdict(list)
    for (face, region), printings in sorted(parents.items()):
        current = currents.get((face, region))
        group = groups.get((face, region))
        if current is not None and group is not None and not group.reasons:
            continue
        candidates = tuple(
            WordingCandidate(printing_id=p, revision_id=r)
            for p in sorted(printings)
            for r in _revision_options(observations[p, face])
        )
        blocked = _correction_blocks(plan, face, region)
        result[face].append(
            WordingView.model_validate(
                {
                    "region": region,
                    "display": _display(current, candidates, dates, blocked, contents),
                    "candidates": candidates,
                    "undated_printing_ids": tuple(
                        p for p in sorted(printings) if dates[p] is None
                    ),
                    **{
                        key: value
                        for key, value in assessments.get((face, region), {}).items()
                        if key
                        in {
                            "classification",
                            "method",
                            "reason",
                            "basis_printing_id",
                            "basis_released_on",
                        }
                    },
                }
            )
        )
    return {face: tuple(items) for face, items in sorted(result.items())}


def _release_basis(
    view: WordingView, decision: WordingDecision | None, dates: dict[str, str | None]
) -> tuple[WordingDisplay, str | None]:
    if decision is not None and decision.basis_printing_id is not None:
        candidates = [
            candidate
            for candidate in view.candidates
            if candidate.printing_id == decision.basis_printing_id
        ]
        if len(candidates) != 1 or candidates[0].revision_id is None:
            raise ValueError("Manual wording basis has no unique observation")
        return WordingDisplay(
            revision_id=candidates[0].revision_id, basis="latest_known_release"
        ), decision.basis_printing_id
    identifiers = [
        candidate.printing_id
        for candidate in view.candidates
        if candidate.revision_id == view.display.revision_id
        and dates[candidate.printing_id] is not None
    ]
    if not identifiers:
        return view.display, None
    latest = max(dates[identifier] or "" for identifier in identifiers)
    return view.display, min(
        identifier for identifier in identifiers if dates[identifier] == latest
    )


def _adopt_basis(
    db: Database,
    face: str,
    view: WordingView,
    display: WordingDisplay,
    classification: Classification,
) -> None:
    if (
        classification != "same_effect"
        or display.revision_id is None
        or display.basis == "current"
    ):
        return
    values: dict[str, Value] = {
        "face_id": face,
        "region": view.region,
        "revision_id": display.revision_id,
        "basis": "latest_known_release",
        "decision_id": None,
    }
    if view.display.basis == "current":
        db.update(
            "face_current",
            {"face_id": face, "region": view.region},
            {
                key: value
                for key, value in values.items()
                if key not in {"face_id", "region"}
            },
        )
    else:
        db.insert("face_current", values)


def _unresolved_reason(view: WordingView, dates: dict[str, str | None]) -> str:
    known = [dates[item.printing_id] for item in view.candidates]
    if not any(known):
        return "no_known_release_day"
    latest = max(day for day in known if day is not None)
    if any(
        item.revision_id is None and dates[item.printing_id] == latest
        for item in view.candidates
    ):
        return "latest_release_missing_source"
    return "latest_release_conflict"


def populate_wording_basis(db: Database, plan: TextPlan) -> None:
    """Select a dated semantic basis only after equivalence or an exact authored decision."""
    dates = printing_dates(db)
    decisions = {(item.face_id, item.region): item for item in plan.wording_decisions}
    views = wording_views(db, plan)
    groups = {(group.face_id, group.region): group for group in plan.groups}
    for face, entries in views.items():
        for view in entries:
            group = groups.get((face, view.region))
            all_contents = (
                ()
                if group is None
                else tuple(item.content for item in group.observations)
            )
            contents = (
                ()
                if group is None
                else tuple(
                    item.content
                    for item in group.observations
                    if dates.get(item.printing_id) is not None
                )
            )
            fingerprint = comparison_hash(all_contents)
            decision = decisions.pop((face, view.region), None)
            classification: Classification = (
                "same_effect" if equivalent(contents) else "pending_review"
            )
            method: Method = "mechanical" if classification == "same_effect" else "none"
            reason = (
                "reminder_or_punctuation"
                if classification == "same_effect"
                else "semantic_review_required"
            )
            if decision is not None:
                if decision.comparison_hash != fingerprint:
                    raise ValueError("Stale wording decision")
                classification, method, reason = (
                    decision.classification,
                    decision.method,
                    decision.reason,
                )
            display, printing = _release_basis(view, decision, dates)
            if (
                classification != "errata"
                and view.display.basis == "current"
                and not (
                    decision is not None and decision.basis_printing_id is not None
                )
            ):
                classification, reason = "unresolved", "adopted_current_preserved"
            if display.revision_id is None and classification != "errata":
                classification, reason = "unresolved", _unresolved_reason(view, dates)
            elif classification != "errata" and (
                group is None
                or any(reason in group.reasons for reason in ("identity_pending",))
                or any(
                    dates[identifier] is not None
                    for identifier in _correction_blocks(plan, face, view.region)
                )
                or any(
                    candidate.revision_id is None
                    and dates[candidate.printing_id] is not None
                    for candidate in view.candidates
                )
            ):
                classification, reason = "unresolved", "incomplete_source"
            db.insert(
                "face_wording",
                {
                    "face_id": face,
                    "region": view.region,
                    "classification": classification,
                    "method": method,
                    "reason": reason,
                    "comparison_hash": fingerprint,
                    "basis_printing_id": printing
                    if classification == "same_effect"
                    else None,
                    "basis_released_on": dates[printing]
                    if printing is not None and classification == "same_effect"
                    else None,
                },
            )
            _adopt_basis(db, face, view, display, classification)
    if decisions:
        raise ValueError("Wording decision has no pending face/region")


def wording_region_blocks(db: Database, plan: TextPlan) -> dict[str, list[JsonValue]]:
    """A missing current on any required face blocks only automatic regional use."""
    views = wording_views(db, plan)
    cards = {
        _string(row.values["id"]): _string(row.values["card_id"])
        for row in db.rows("face")
    }
    missing: dict[str, set[str]] = defaultdict(set)
    for face, items in views.items():
        for item in items:
            if item.display.basis != "current":
                missing[cards[face]].add(item.region)
    return {
        card: [
            {"region": region, "reasons": ["wording_pending"]}
            for region in sorted(regions)
        ]
        for card, regions in sorted(missing.items())
    }


def printing_observed_texts(
    db: Database, plan: TextPlan
) -> dict[tuple[str, str], tuple[ObservedText, ...]]:
    """Project each printing's own observations, retaining unknown main text."""
    revisions = {_string(row.values["id"]) for row in db.rows("face_revision")}
    included = {
        record.data.id
        for record in plan.publication_identity().included("printing")
        if isinstance(record.data, PrintingData)
    }
    result: dict[tuple[str, str], list[ObservedText]] = defaultdict(list)
    for item in plan.candidates():
        if item.printing_id not in included:
            continue
        revision = None if item.content.effect is None else candidate_revision_id(item)
        if revision is not None and revision not in revisions:
            raise ValueError("Printing observation revision is missing")
        result[item.printing_id, item.face_id].append(
            ObservedText(
                revision_id=revision,
                state="available" if revision is not None else "missing_effect",
                source_url=item.card.source.url,
            )
        )
    for record in plan.publication_identity().included("printing"):
        printing = record.data
        if (
            not isinstance(printing, PrintingData)
            or printing.id not in plan.unavailable
        ):
            continue
        source = plan.identity.evidence[printing.region, printing.card_no].source
        for mapping in printing.source_face_map:
            result[printing.id, mapping.face_id].append(
                ObservedText(
                    revision_id=None, state="missing_effect", source_url=source.url
                )
            )
    return {
        key: _canonical_observations(items) for key, items in sorted(result.items())
    }


def _canonical_observations(items: list[ObservedText]) -> tuple[ObservedText, ...]:
    return tuple(
        sorted(
            set(items),
            key=lambda item: (item.source_url, item.state, item.revision_id or ""),
        )
    )


def mark_wording_pending(db: Database, plan: TextPlan) -> None:
    """Add the build-side block to existing support rows without altering deck identity."""
    blocked = wording_region_blocks(db, plan)
    for row in db.rows("card_engine_support"):
        values = row.values
        regions = {
            string(item["region"])
            for item in blocked.get(_string(values["card_id"]), [])
            if isinstance(item, dict)
        }
        if values["region"] not in regions:
            continue
        raw = values["reason_codes"]
        if not isinstance(raw, Json) or not isinstance(raw.value, list):
            raise TypeError("Invalid support reasons")
        reasons = sorted({string(reason) for reason in raw.value} | {"wording_pending"})
        changes: dict[str, Value] = {
            "reason_codes": Json(list[JsonValue](reasons)),
            "automatic": False,
        }
        db.update(
            "card_engine_support",
            {"card_id": values["card_id"], "region": values["region"]},
            changes,
        )
