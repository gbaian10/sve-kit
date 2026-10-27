"""Keep reviewed token distinctions and alias adoption from regressing in public inputs."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.domains.digital.links.loader import load_links
from sve_carddb.domains.registry.records import PrintingData

from ...support.adoption_fixtures import REPO

if TYPE_CHECKING:
    from sve_carddb.domains.digital.links.models import Record
    from sve_carddb.domains.registry.snapshot import RegistrySnapshot


@pytest.fixture(scope="module")
def adopted() -> tuple[Record, ...]:
    return load_links(REPO / "authored").records()


@pytest.fixture(scope="module")
def card_numbers(official_snapshot: RegistrySnapshot) -> dict[str, str]:
    return {
        record.data.card_no: record.data.card_id
        for record in official_snapshot.records.values()
        if isinstance(record.data, PrintingData) and record.data.region == "jp"
    }


@pytest.mark.parametrize(
    ("game", "official", "number", "relation"),
    [
        ("svwb", "90032110", "BP09-049", "same_character"),
        ("svwb", "90001110", "BP01-171", "same_character"),
        ("svwb", "90034340", "BP07-041", "same_character"),
        ("svwb", "90024330", "BP11-028", "same_character"),
        ("svwb", "90021130", "BP11-031", "same_character"),
        ("svwb", "90044310", "BP06-061", "same_character"),
        ("svwb", "90023110", "BP09-023", "same_character"),
        ("sv1", "900644090", "BP15-PR14", "same_card"),
        ("sv1", "900244050", "BP15-PR10", "same_card"),
        ("svwb", "90051120", "BP16-T19", "same_card"),
        ("svwb", "90061120", "BP16-T20", "same_card"),
        ("svwb", "90041120", "BP16-T17", "same_card"),
    ],
)
def test_reviewed_reclassifications_and_aliases(
    adopted: tuple[Record, ...],
    card_numbers: dict[str, str],
    game: str,
    official: str,
    number: str,
    relation: str,
) -> None:
    matches = [
        record
        for record in adopted
        if record.subject.game == game
        and record.subject.official_id == official
        and record.subject.card_id == card_numbers[number]
    ]
    assert len(matches) == 1
    assert matches[0].value.relation == relation


def test_adoption_never_claims_human_or_exact_face_review(
    adopted: tuple[Record, ...],
) -> None:
    assert adopted
    assert {record.review_level for record in adopted} == {"model_reviewed"}
    assert all(
        record.subject.face_id is None and record.subject.digital_phase is None
        for record in adopted
    )
    assert not any(
        record.subject.game == "sv1" and record.subject.official_id == "900544010"
        for record in adopted
    )
