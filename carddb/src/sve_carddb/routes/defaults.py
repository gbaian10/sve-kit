"""Default-printing ordering over explicit classification evidence and build dates."""

from collections import defaultdict
from dataclasses import dataclass
from datetime import date
from typing import TYPE_CHECKING, Literal

from sve_carddb.routes.plan import confirmed, text

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.build_db.database import Database, Row


@dataclass(frozen=True)
class GeneralEvidence:
    """Facts from an adopted classifier; absent facts remain unknown, never ordinary."""

    general_rarity: bool | None = None
    ordinary_frame: bool | None = None
    stamped: bool | None = None

    def __post_init__(self) -> None:
        """Refuse truthy values masquerading as reviewed facts."""
        for value in (self.general_rarity, self.ordinary_frame, self.stamped):
            if value is not None and type(value) is not bool:
                raise TypeError("General-printing evidence must be bool or unknown")


@dataclass(frozen=True)
class DefaultPrinting:
    card_id: str
    region: str
    printing_id: str
    method: Literal["override", "fallback", "earliest_general", "candidate_general"]


@dataclass(frozen=True)
class _Candidate:
    printing_id: str
    first_date: str | None
    dates_complete: bool
    general: bool
    ordinary_complete: bool
    classification_unknown: bool

    @property
    def order(self) -> tuple[bool, str, str]:
        return self.first_date is None, self.first_date or "", self.printing_id


def _inclusion_date(inclusion: Row, product: Row) -> str | None:
    precision = inclusion.values["first_available_precision"]
    value = inclusion.values["first_available_on"]
    if precision is None:
        precision = product.values["date_precision"]
        value = product.values["released_on"]
    if precision != "day" or value is None:
        return None
    if not isinstance(value, str):
        raise TypeError("Expected inclusion date")
    date.fromisoformat(value)
    return value


def _dates(db: Database) -> dict[str, tuple[str | None, bool]]:
    products = {text(row, "id"): row for row in db.rows("product")}
    printings = {text(row, "id"): text(row, "region") for row in db.rows("printing")}
    dates: dict[str, list[str | None]] = defaultdict(list)
    for row in db.rows("printing_product"):
        pid = text(row, "printing_id")
        product = products[text(row, "product_id")]
        if printings[pid] != text(product, "region"):
            raise ValueError("Default inclusion must match the printing region")
        dates[pid].append(_inclusion_date(row, product))
    return {
        pid: (
            min(known) if (known := [v for v in values if v is not None]) else None,
            all(v is not None for v in values),
        )
        for pid, values in dates.items()
    }


def _candidate(
    row: Row,
    home: str,
    faces: tuple[Row, ...],
    evidence: GeneralEvidence,
    dates: tuple[str | None, bool],
    faces_complete: bool,
) -> _Candidate:
    premium = row.values["premium"]
    eligible = (
        text(row, "home_set_id") == home
        and premium is not True
        and not any(face.values["signed"] is True for face in faces)
        and evidence.stamped is not True
        and evidence.ordinary_frame is not False
    )
    general = eligible and evidence.general_rarity is True
    ordinary_complete = (
        premium is False
        and bool(faces)
        and faces_complete
        and all(face.values["signed"] is False for face in faces)
        and evidence.stamped is False
        and evidence.ordinary_frame is True
        and all(
            face.values["embellishment_state"] in {"sampled", "confirmed"}
            for face in faces
        )
    )
    return _Candidate(
        text(row, "id"),
        *dates,
        general,
        ordinary_complete,
        eligible and evidence.general_rarity is None,
    )


def select_defaults(
    db: Database,
    *,
    general_evidence: Mapping[str, GeneralEvidence] | None = None,
) -> tuple[DefaultPrinting, ...]:
    """Select per card/region; unspecified rarity/frame/stamp policy yields fallback."""
    evidence = {} if general_evidence is None else general_evidence
    printings = db.rows("printing")
    if set(evidence) - {text(row, "id") for row in printings}:
        raise ValueError("General evidence references an absent printing")
    homes = {text(row, "id"): text(row, "home_set_id") for row in db.rows("card")}
    card_faces: dict[str, set[str]] = defaultdict(set)
    for row in db.rows("face"):
        card_faces[text(row, "card_id")].add(text(row, "id"))
    faces: dict[str, list[Row]] = defaultdict(list)
    for row in db.rows("printing_face"):
        faces[text(row, "printing_id")].append(row)
    dates = _dates(db)
    groups: dict[tuple[str, str], list[_Candidate]] = defaultdict(list)
    for row in printings:
        pid, cid = text(row, "id"), text(row, "card_id")
        groups[cid, text(row, "region")].append(
            _candidate(
                row,
                homes[cid],
                tuple(faces[pid]),
                evidence.get(pid, GeneralEvidence()),
                dates.get(pid, (None, False)),
                {text(face, "face_id") for face in faces[pid]} == card_faces[cid],
            )
        )
    overrides = {
        (text(row, "card_id"), text(row, "region")): row
        for row in db.rows("default_printing_override")
    }
    if overrides.keys() - groups.keys():
        raise ValueError("Default override has no card/region printing")
    return tuple(
        _select(db, key, candidates, overrides.get(key))
        for key, candidates in sorted(groups.items())
    )


def _select(
    db: Database,
    key: tuple[str, str],
    candidates: list[_Candidate],
    override: Row | None,
) -> DefaultPrinting:
    if override is not None:
        confirmed(db, text(override, "decision_id"))
        selected = text(override, "printing_id")
        if selected not in {item.printing_id for item in candidates}:
            raise ValueError("Default override must select the same card and region")
        return DefaultPrinting(*key, selected, "override")
    general = [item for item in candidates if item.general]
    if not general:
        selected_candidate = min(candidates, key=lambda item: item.order)
        return DefaultPrinting(*key, selected_candidate.printing_id, "fallback")
    selected_candidate = min(general, key=lambda item: item.order)
    complete = all(
        item.dates_complete and item.ordinary_complete for item in general
    ) and not any(item.classification_unknown for item in candidates)
    return DefaultPrinting(
        *key,
        selected_candidate.printing_id,
        "earliest_general" if complete else "candidate_general",
    )
