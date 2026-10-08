"""Resolve exact adopted printings and retain unresolvable evidence as staging."""

from collections import defaultdict
from dataclasses import dataclass
from typing import TYPE_CHECKING
from urllib.parse import urljoin

from pydantic import JsonValue

from sve_carddb.card_extras.archive import card_number
from sve_carddb.card_extras.models import CardPage, ErrataPage, QAEntry, QAPage, key
from sve_carddb.core.json import canonical
from sve_carddb.core.provenance import SourceUse, uses_sorted

if TYPE_CHECKING:
    from collections.abc import Iterable, Mapping

    from sve_carddb.build import Database, Value
    from sve_carddb.registry.records import Region


@dataclass(frozen=True)
class QAVersion:
    qa_id: str
    id: str
    revision: int
    entry: QAEntry
    pages: tuple[CardPage | QAPage, ...]
    card_ids: tuple[str, ...]


@dataclass(frozen=True)
class Relation:
    id: str
    from_card_id: str
    to_card_id: str
    target_printing_id: str
    page: CardPage


@dataclass(frozen=True)
class Gap:
    category: str
    region: Region
    card_no: str
    card_id: str | None
    source_id: str
    locator: str
    target: str | None = None

    def report(self) -> dict[str, JsonValue]:
        """Expose identifiers and missing evidence, never Q&A or card text."""
        return {
            "category": self.category,
            "region": self.region,
            "card_no": self.card_no,
            "card_id": self.card_id,
            "source_id": self.source_id,
            "locator": self.locator,
            "target": self.target,
        }


@dataclass(frozen=True)
class ExtrasPlan:
    pages: tuple[CardPage, ...]
    errata: tuple[ErrataPage, ...]
    questions: tuple[QAVersion, ...]
    relations: tuple[Relation, ...]
    gaps: tuple[Gap, ...]
    qa_pages: tuple[QAPage, ...] = ()

    def source_uses(self) -> tuple[SourceUse, ...]:
        """Keep evidence closure even for duplicates, unknown targets and empty pages."""
        uses: list[SourceUse] = []
        for page in self.pages:
            uses.append(
                SourceUse(
                    source=page.source,
                    usage="card_extras_page",
                    locator=canonical(
                        {"region": page.region, "card_no": page.card_no}
                    ).decode(),
                )
            )
            uses.extend(
                SourceUse(source=page.source, usage="official_qa", locator=qa.locator)
                for qa in page.qa
            )
            uses.extend(
                SourceUse(
                    source=page.source,
                    usage="official_related",
                    locator=link.locator,
                )
                for link in page.related
            )
            uses.extend(
                SourceUse(source=page.source, usage="errata_reference", locator=url)
                for url in page.errata_urls
            )
        for qa_page in self.qa_pages:
            uses.append(
                SourceUse(
                    source=qa_page.source,
                    usage="qa_page",
                    locator=qa_page.source.url,
                )
            )
            for block in qa_page.blocks:
                uses.append(
                    SourceUse(
                        source=qa_page.source,
                        usage="official_qa",
                        locator=block.entry.locator,
                    )
                )
                uses.extend(
                    SourceUse(
                        source=qa_page.source,
                        usage="qa_card_reference",
                        locator=link.locator,
                    )
                    for link in block.card_links
                )
        for notice in self.errata:
            uses.append(
                SourceUse(
                    source=notice.source,
                    usage="official_errata",
                    locator=notice.official_url,
                )
            )
            uses.extend(
                SourceUse(
                    source=notice.source,
                    usage="official_errata_change",
                    locator=change.locator,
                )
                for change in notice.changes
            )
        return uses_sorted(uses)

    def configuration(self) -> dict[str, JsonValue]:
        """Pin input semantics by hash, without persisting official content in reports."""
        result: dict[str, JsonValue] = {
            "recipe": "card-extras-v1",
            "pages": [key("page", page.model_dump(mode="json")) for page in self.pages],
            "errata": [
                key("notice", page.model_dump(mode="json")) for page in self.errata
            ],
        }
        if self.qa_pages:
            result["qa_pages"] = [
                key("qa-page", page.model_dump(mode="json")) for page in self.qa_pages
            ]
        return result

    def report(self) -> dict[str, JsonValue]:
        """Keep production counts separate from caller-owned synthetic test counts."""
        return {
            "pages": len(self.pages),
            "qa": len({v.qa_id for v in self.questions}),
            "qa_versions": len(self.questions),
            "qa_cards": sum(len(v.card_ids) for v in self.questions),
            "related": len(self.relations),
            "errata_sources": len(self.errata),
            "staging": [gap.report() for gap in self.gaps],
            "source_windows": [],
        }


def printing_index(db: Database) -> dict[tuple[str, str], Mapping[str, Value]]:
    """Require an unambiguous exact regional number; no suffix or variant guessing."""
    result: dict[tuple[str, str], Mapping[str, Value]] = {}
    for row in db.rows("printing"):
        region, number = row.values["region"], row.values["card_no"]
        if not isinstance(region, str) or not isinstance(number, str):
            continue
        lookup = region, number
        if lookup in result:
            raise ValueError("Ambiguous adopted regional card number")
        result[lookup] = row.values
    return result


def plan_card_extras(
    db: Database,
    pages: Iterable[CardPage],
    *,
    errata: tuple[ErrataPage, ...] = (),
    qa_pages: Iterable[QAPage] = (),
) -> ExtrasPlan:
    """Create deterministic offline staging against an already adopted identity graph."""
    checked = tuple(
        sorted(
            {CardPage.model_validate_json(page.model_dump_json()) for page in pages},
            key=lambda page: (
                page.region,
                page.card_no,
                page.source.fetched_at,
                page.source.id,
            ),
        )
    )
    notices = tuple(
        sorted(
            {
                page.model_dump_json(): ErrataPage.model_validate_json(
                    page.model_dump_json()
                )
                for page in errata
            }.values(),
            key=lambda notice: (
                notice.region,
                notice.official_url,
                notice.source.fetched_at,
                notice.source.id,
            ),
        )
    )
    printings = printing_index(db)
    questions: dict[str, list[tuple[QAEntry, CardPage | QAPage, str | None]]] = (
        defaultdict(list)
    )
    relations: dict[str, Relation] = {}
    gaps: list[Gap] = []
    for page in checked:
        if (
            card_number(page.source.url, page.region) != page.card_no
            or page.source.kind != "official_page"
        ):
            raise ValueError("Card extras page identity mismatch")
        printing = printings.get((page.region, page.card_no))
        card_id = None if printing is None else str(printing["card_id"])
        if card_id is None:
            gaps.append(
                Gap(
                    "source_printing_missing",
                    page.region,
                    page.card_no,
                    None,
                    page.source.id,
                    "page",
                )
            )
        for entry in page.qa:
            if card_id is not None:
                questions[entry.identity(page.region)].append((entry, page, card_id))
        _relations(page, card_id, printings, relations, gaps)
        # An announcement fragment is insufficient to adjudicate the current card text.
        gaps.extend(
            Gap(
                "errata_current_pending",
                page.region,
                page.card_no,
                card_id,
                page.source.id,
                "errata-reference",
                url,
            )
            for url in page.errata_urls
        )
    checked_qa = tuple(
        sorted(
            {QAPage.model_validate_json(page.model_dump_json()) for page in qa_pages},
            key=lambda page: (page.region, page.source.fetched_at, page.source.id),
        )
    )
    _qa_observations(checked_qa, printings, questions, gaps)
    for notice in notices:
        numbers = {item.card_no for item in notice.printings} | {
            item.card_no for item in notice.changes
        }
        for number in sorted(numbers):
            printing = printings.get((notice.region, number))
            gaps.append(
                Gap(
                    "errata_current_pending",
                    notice.region,
                    number,
                    None if printing is None else str(printing["card_id"]),
                    notice.source.id,
                    "errata-announcement",
                    notice.official_url,
                )
            )
    versions = _qa_versions(questions)
    return ExtrasPlan(
        checked,
        notices,
        versions,
        tuple(relations[name] for name in sorted(relations)),
        tuple(gaps),
        checked_qa,
    )


def _qa_observations(
    pages: tuple[QAPage, ...],
    printings: dict[tuple[str, str], Mapping[str, Value]],
    questions: dict[str, list[tuple[QAEntry, CardPage | QAPage, str | None]]],
    gaps: list[Gap],
) -> None:
    for page in pages:
        for block in page.blocks:
            cards: set[str] = set()
            for link in block.card_links:
                url = urljoin(page.source.url, link.href_raw)
                number = card_number(url, page.region)
                printing = (
                    None if number is None else printings.get((page.region, number))
                )
                if printing is None:
                    gaps.append(
                        Gap(
                            "qa_target_missing"
                            if number is not None
                            else "qa_link_unrecognized",
                            page.region,
                            number or "",
                            None,
                            page.source.id,
                            link.locator,
                            url,
                        )
                    )
                else:
                    cards.add(str(printing["card_id"]))
            for card in sorted(cards) if cards else [None]:
                questions[block.entry.identity(page.region)].append(
                    (block.entry, page, card)
                )


def _relations(
    page: CardPage,
    card_id: str | None,
    printings: dict[tuple[str, str], Mapping[str, Value]],
    relations: dict[str, Relation],
    gaps: list[Gap],
) -> None:
    for link in page.related:
        url = urljoin(page.source.url, link.href_raw)
        target_no = card_number(url, page.region)
        target = (
            printings.get((page.region, target_no)) if target_no is not None else None
        )
        if card_id is None or target is None:
            gaps.append(
                Gap(
                    "related_target_missing"
                    if target_no is not None
                    else "related_link_unrecognized",
                    page.region,
                    page.card_no,
                    card_id,
                    page.source.id,
                    link.locator,
                    url,
                )
            )
        elif target["card_id"] == card_id:
            gaps.append(
                Gap(
                    "related_same_card",
                    page.region,
                    page.card_no,
                    card_id,
                    page.source.id,
                    link.locator,
                    url,
                )
            )
        else:
            identity = key(
                "related",
                [page.source.id, card_id, str(target["id"]), "official_unspecified"],
            )
            relations[identity] = Relation(
                identity, card_id, str(target["card_id"]), str(target["id"]), page
            )


def _qa_versions(
    questions: dict[str, list[tuple[QAEntry, CardPage | QAPage, str | None]]],
) -> tuple[QAVersion, ...]:
    versions: list[QAVersion] = []
    for owner, entries in sorted(questions.items()):
        episodes: list[list[tuple[QAEntry, CardPage | QAPage, str | None]]] = []
        for observed in sorted(
            entries,
            key=lambda item: (
                item[1].source.fetched_at,
                item[1].source.id,
                item[0].locator,
            ),
        ):
            if (
                not episodes
                or observed[0].fingerprint() != episodes[-1][0][0].fingerprint()
            ):
                episodes.append([])
            episodes[-1].append(observed)
        seen: set[str] = set()
        for revision, episode in enumerate(episodes):
            entry, page, _ = episode[0]
            fingerprint = entry.fingerprint()
            identity: list[JsonValue] = [owner, fingerprint]
            if fingerprint in seen:
                identity.extend((page.source.id, entry.locator))
            seen.add(fingerprint)
            versions.append(
                QAVersion(
                    owner,
                    key("qav", identity),
                    revision,
                    entry,
                    tuple(item[1] for item in episode),
                    tuple(sorted({item[2] for item in episode if item[2] is not None})),
                )
            )
    return tuple(versions)
