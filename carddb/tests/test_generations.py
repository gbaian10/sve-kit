import sqlite3

import pytest

from sve_carddb.ingest.archive.manifest import (
    Edge,
    GenerationStatus,
    Kind,
    Link,
    Manifest,
    ManifestError,
)

ROOT = "jp:list:BP15"
BASE = "https://shadowverse-evolve.com/cardlist/cardsearch_ex?expansion_name=BP15&view=text&page="


def page(n: int) -> str:
    return f"{BASE}{n}"


def card(number: str) -> Link:
    url = f"https://shadowverse-evolve.com/cardlist/?cardno={number}"
    return Link(url, Kind.CARD, 0, number)


def discover(manifest: Manifest, pages: dict[int, list[str]], *, validate: bool) -> int:
    generation = manifest.generations.start(ROOT)
    for n, numbers in pages.items():
        links = [
            Link(card(num).to_url, Kind.CARD, i, num) for i, num in enumerate(numbers)
        ]
        with manifest.transaction():
            manifest.generations.add_page(generation.id, page(n), f"sha-{n}", links)
    if validate:
        manifest.generations.validate(
            generation.id, declared_total=sum(map(len, pages.values()))
        )
    return generation.id


def card_numbers(manifest: Manifest) -> list[str]:
    current = manifest.generations.current(ROOT)
    assert current is not None
    return [edge.link.original for edge in manifest.generations.edges(current.id)]


def test_unvalidated_generation_is_not_current(manifest: Manifest) -> None:
    discover(manifest, {1: ["BP15-001"]}, validate=False)
    assert manifest.generations.current(ROOT) is None


def test_validated_generation_exposes_its_edges(manifest: Manifest) -> None:
    generation_id = discover(manifest, {1: ["BP15-001", "BP15-002"]}, validate=True)
    current = manifest.generations.current(ROOT)
    assert current is not None
    assert current.id == generation_id
    assert current.declared_total == 2
    assert manifest.generations.edges(generation_id) == [
        Edge(page(1), Link(card("BP15-001").to_url, Kind.CARD, 0, "BP15-001")),
        Edge(page(1), Link(card("BP15-002").to_url, Kind.CARD, 1, "BP15-002")),
    ]


def test_interrupted_refresh_keeps_the_old_generation_intact(
    manifest: Manifest,
) -> None:
    discover(manifest, {1: ["BP15-001"], 2: ["BP15-016"]}, validate=True)
    discover(manifest, {1: ["BP15-NEW"]}, validate=False)
    assert card_numbers(manifest) == ["BP15-001", "BP15-016"]


def test_new_start_fails_the_unfinished_generation(manifest: Manifest) -> None:
    unfinished = discover(manifest, {1: ["BP15-001"]}, validate=False)
    manifest.generations.start(ROOT)
    assert manifest.generations.get(unfinished).status is GenerationStatus.FAILED


def test_validation_supersedes_the_previous_generation(manifest: Manifest) -> None:
    old = discover(
        manifest, {1: ["BP15-001"], 2: ["BP15-016"], 3: ["BP15-031"]}, validate=True
    )
    discover(manifest, {1: ["BP15-001"], 2: ["BP15-016"]}, validate=True)
    assert manifest.generations.get(old).status is GenerationStatus.SUPERSEDED
    assert card_numbers(manifest) == ["BP15-001", "BP15-016"]


def test_a_page_is_recorded_once_per_generation(manifest: Manifest) -> None:
    generation = manifest.generations.start(ROOT)
    with manifest.transaction():
        manifest.generations.add_page(generation.id, page(1), "sha-1", [])
    with pytest.raises(sqlite3.IntegrityError), manifest.transaction():
        manifest.generations.add_page(generation.id, page(1), "sha-recheck", [])


def test_finished_generations_are_immutable(manifest: Manifest) -> None:
    generation_id = discover(manifest, {1: ["BP15-001"]}, validate=True)
    with pytest.raises(ManifestError, match="validated"), manifest.transaction():
        manifest.generations.add_page(generation_id, page(2), "sha-2", [])


def test_failed_generation_is_never_current(manifest: Manifest) -> None:
    generation_id = discover(manifest, {1: ["BP15-001"]}, validate=False)
    manifest.generations.fail(generation_id)
    assert manifest.generations.get(generation_id).status is GenerationStatus.FAILED
    assert manifest.generations.current(ROOT) is None
    with pytest.raises(ManifestError, match="failed"):
        manifest.generations.validate(generation_id)
