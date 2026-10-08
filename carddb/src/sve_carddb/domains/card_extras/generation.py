"""Recompute Q&A closure from observed pages, never from card-list coverage."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.core.json import digest
from sve_carddb.ingest.archive.manifest import Kind, Link

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.core.regions import Region
    from sve_carddb.domains.card_extras.qa_parser import ParsedQA


@dataclass(frozen=True)
class Observation:
    parsed: ParsedQA
    sha256: str


@dataclass(frozen=True)
class Closure:
    issues: tuple[str, ...]
    total: int | None
    max_page: int | None

    @property
    def complete(self) -> bool:
        """Require every independent closure check to succeed."""
        return not self.issues


def root_key(url: str, region: Region) -> str:
    """Keep a root's scope separate from regional card-list generations."""
    return f"{region}:qa:{digest(url.encode())}"


def links(parsed: ParsedQA) -> list[Link]:
    """Snapshot exact pagination and detail edges, retaining repeated associations."""
    result = [
        Link(link.url, Kind.QA, position, link.href_raw)
        for position, link in enumerate(parsed.details)
    ]
    if parsed.pagination is not None:
        result.extend(
            Link(url, Kind.LIST, number, str(number))
            for number, url in parsed.pagination.urls
        )
    return result


def closure(root: str, pages: Mapping[str, Observation]) -> Closure:
    """Validate explicit counts and reachability independently of manifest status."""
    first = pages.get(root)
    if first is None or first.parsed.pagination is None:
        return Closure(("root_missing",), None, None)
    pagination = first.parsed.pagination
    maximum, total = pagination.max_page, pagination.total
    issues: list[str] = []
    if maximum is None or total is None or pagination.page != 1:
        return Closure(("pagination_unknown",), total, maximum)
    targets = dict(pagination.urls)
    if set(targets) != set(range(1, maximum + 1)) or targets.get(1) != root:
        issues.append("pagination_gap")
    count = 0
    expected = set(targets.values())
    for number, url in targets.items():
        observed = pages.get(url)
        if observed is None:
            issues.append("index_unfetched")
            continue
        issues.extend(_index_issues(observed, number, maximum, total, targets))
        count += observed.parsed.listed_count
        for detail in observed.parsed.details:
            expected.add(detail.url)
            issues.extend(_detail_issues(pages.get(detail.url)))
    if count != total:
        issues.append("total_disagreement")
    if not set(pages).issubset(expected):
        issues.append("generation_membership_disagreement")
    return Closure(tuple(sorted(set(issues))), total, maximum)


def _detail_issues(target: Observation | None) -> list[str]:
    if target is None:
        return ["detail_unfetched"]
    if target.parsed.pagination is not None or not target.parsed.blocks:
        return ["detail_invalid"]
    return []


def _index_issues(
    observed: Observation,
    number: int,
    maximum: int,
    total: int,
    targets: dict[int, str],
) -> list[str]:
    issues: list[str] = []
    current = observed.parsed.pagination
    if current is None or (current.page, current.max_page, current.total) != (
        number,
        maximum,
        total,
    ):
        issues.append("pagination_disagreement")
    elif any(targets.get(n) != target for n, target in current.urls):
        issues.append("pagination_target_disagreement")
    if "unknown_empty_layout" in observed.parsed.issues:
        issues.append("empty_layout_unknown")
    return issues
