"""Read protected QA/card history and independently verify frozen generation closure."""

from dataclasses import dataclass
from pathlib import PurePosixPath
from typing import TYPE_CHECKING, Literal, cast

from pydantic import JsonValue

from sve_carddb.core.json import digest
from sve_carddb.domains.card_extras.archive import EN_PARSER, parse_card_page
from sve_carddb.domains.card_extras.archive import PARSER as CARD_PARSER
from sve_carddb.domains.card_extras.changes import conflicts
from sve_carddb.domains.card_extras.generation import (
    Closure,
    Observation,
    closure,
    links,
    root_key,
)
from sve_carddb.domains.card_extras.qa_parser import PARSER, materialize, parse_qa
from sve_carddb.ingest.archive.frozen_sources import FrozenSources
from sve_carddb.ingest.archive.manifest import GenerationStatus, Manifest
from sve_carddb.ingest.archive.store import resolve_within
from sve_carddb.parse.pages.official_qa import allowed

if TYPE_CHECKING:
    from collections.abc import Iterator
    from pathlib import Path

    from sve_carddb.core.regions import Region
    from sve_carddb.domains.card_extras.models import CardPage, QAPage
    from sve_carddb.ingest.archive.manifest import Link


@dataclass(frozen=True)
class FrozenCoverage:
    generation_id: int | None
    closure: Closure
    source_ids: tuple[str, ...]


class FrozenOfficialExtras:
    def __init__(
        self, root: Path, store_id: str, batch_id: str, *, region: Region
    ) -> None:
        self.sources = FrozenSources(root, store_id, batch_id)
        self.region = region

    def qa_pages(self) -> Iterator[QAPage]:
        """Retain all archived versions, including unassociated and withdrawn blocks."""
        for entry in self.sources.inventory.entries:
            descriptor = self.sources.descriptor(entry.source_version_id)
            if descriptor.provider != self.region or not allowed(
                descriptor.url, self.region
            ):
                continue
            kind = _kind(descriptor.kind)
            source, raw, _ = self.sources.read(
                entry.source_version_id, parser_version=PARSER
            )
            yield materialize(raw, source, region=self.region, kind=kind)

    def card_pages(self) -> Iterator[CardPage]:
        """Use the shared regional supplemental parser for every archived card version."""
        parser = CARD_PARSER if self.region == "jp" else EN_PARSER
        for entry in self.sources.inventory.entries:
            descriptor = self.sources.descriptor(entry.source_version_id)
            if (descriptor.provider, descriptor.kind) != (self.region, "card"):
                continue
            source, raw, _ = self.sources.read(
                entry.source_version_id, parser_version=parser
            )
            yield parse_card_page(raw, source, region=self.region)

    def generation_pages(self, root: str) -> tuple[QAPage, ...]:
        """Scope reconciliation to the latest attempt's verified frozen members."""
        return self._pages(self.coverage(root).source_ids)

    def _pages(self, ids: tuple[str, ...]) -> tuple[QAPage, ...]:
        pages = []
        for version in ids:
            source, raw, descriptor = self.sources.read(version, parser_version=PARSER)
            pages.append(
                materialize(
                    raw, source, region=self.region, kind=_kind(descriptor.kind)
                )
            )
        return tuple(pages)

    def report(self, root: str) -> dict[str, JsonValue]:
        """Expose coverage and conflicts together before any current-wording selection."""
        checked = self.coverage(root)
        return {
            "region": self.region,
            "root": root,
            "generation_id": checked.generation_id,
            "complete": checked.closure.complete,
            "issues": [cast("JsonValue", issue) for issue in checked.closure.issues],
            "total": checked.closure.total,
            "max_page": checked.closure.max_page,
            "source_ids": [
                cast("JsonValue", version) for version in checked.source_ids
            ],
            "conflicts": [
                cast("JsonValue", item)
                for item in conflicts(self._pages(checked.source_ids))
            ],
        }

    def coverage(self, root: str) -> FrozenCoverage:
        """A validated flag cannot replace frozen hashes, edges and closure evidence."""
        if not allowed(root, self.region):
            raise ValueError("Q&A coverage root/region mismatch")
        manifest_path = resolve_within(
            self.sources.root,
            PurePosixPath("batches", self.sources.batch_id[7:], "manifest.sqlite"),
        )
        if digest(manifest_path.read_bytes()) != self.sources.inventory.manifest.sha256:
            raise ValueError("Frozen Q&A manifest hash mismatch")
        with Manifest.open_snapshot(manifest_path) as manifest:
            generation = manifest.generations.latest(root_key(root, self.region))
            if generation is None:
                return FrozenCoverage(
                    None, Closure(("generation_unknown",), None, None), ()
                )
            members = self._members()
            edges = manifest.generations.edges(generation.id)
            pages: dict[str, Observation] = {}
            ids: list[str] = []
            issues: list[str] = []
            for member in manifest.generations.pages(generation.id):
                version = members.get((member.url, member.sha256))
                if version is None:
                    issues.append("generation_source_unfrozen")
                    continue
                source, raw, descriptor = self.sources.read(
                    version, parser_version=PARSER
                )
                parsed = parse_qa(
                    raw, url=source.url, region=self.region, kind=_kind(descriptor.kind)
                )
                pages[source.url] = Observation(parsed, member.sha256)
                ids.append(source.id)
                expected = links(parsed)
                actual = [edge.link for edge in edges if edge.from_url == source.url]
                if sorted(actual, key=_edge_key) != sorted(expected, key=_edge_key):
                    issues.append("generation_edges_disagreement")
            checked = closure(root, pages)
            issues.extend(checked.issues)
            if generation.status is not GenerationStatus.VALIDATED:
                issues.append("generation_not_validated")
            if (generation.declared_total, generation.max_page) != (
                checked.total,
                checked.max_page,
            ):
                issues.append("generation_metadata_disagreement")
            return FrozenCoverage(
                generation.id,
                Closure(tuple(sorted(set(issues))), checked.total, checked.max_page),
                tuple(sorted(ids)),
            )

    def _members(self) -> dict[tuple[str, str], str]:
        result: dict[tuple[str, str], str] = {}
        for entry in self.sources.inventory.entries:
            descriptor = self.sources.descriptor(entry.source_version_id)
            if descriptor.provider == self.region:
                previous = result.get((descriptor.url, descriptor.raw_sha256[7:]))
                if previous is not None and previous != descriptor.id:
                    raise ValueError("Ambiguous frozen Q&A source identity")
                result[descriptor.url, descriptor.raw_sha256[7:]] = descriptor.id
        return result


def _kind(kind: str) -> Literal["index", "detail"]:
    if kind not in {"list", "qa"}:
        raise ValueError("Q&A source must explicitly distinguish index from detail")
    return "index" if kind == "list" else "detail"


def _edge_key(link: Link) -> tuple[str, str, int, str]:
    return link.to_url, link.to_kind.value, link.position, link.original
