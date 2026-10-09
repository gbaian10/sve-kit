from dataclasses import asdict, dataclass, replace
from typing import TYPE_CHECKING, cast

import pytest

from sve_carddb.domains.registry.inputs import Card, Face
from sve_carddb.domains.registry.projection import jp_card
from sve_carddb.domains.registry.review import observation
from sve_carddb.domains.text_observations.archive import jp_face
from sve_carddb.ingest.http.validate import ValidationError
from sve_carddb.parse.pages.extract_jp import extract_card

if TYPE_CHECKING:
    from sve_carddb.parse.pages.extract_jp import CardRecord


@dataclass(frozen=True)
class TraitCase:
    raw: str
    complete: tuple[str, ...]


CASES = (
    TraitCase("-", ()),
    TraitCase("Alpha・Beta", ("Alpha", "Beta")),
    TraitCase("ジオ・テオゴニア", ("ジオ・テオゴニア",)),
    TraitCase("〈Synthetic・Compound〉", ("〈Synthetic・Compound〉",)),
    TraitCase(
        "Alpha・〈Synthetic・Compound〉・Beta",
        ("Alpha", "〈Synthetic・Compound〉", "Beta"),
    ),
    TraitCase(
        "〈First・Group〉・〈Second・Group〉", ("〈First・Group〉", "〈Second・Group〉")
    ),
    TraitCase("〈Three・Part・Group〉", ("〈Three・Part・Group〉",)),
    TraitCase("〈Single〉・Alpha", ("〈Single〉", "Alpha")),
)


def page(raw: str) -> bytes:
    faces = [
        f"""<div class="cardlist-Detail_Box_Inner">
        <div class="img"><img src="/synthetic.png"></div><h1 class="ttl">{name}</h1>
        <div class="info">
        <dl><dt>クラス</dt><dd>Synthetic class</dd></dl>
        <dl><dt>カード種類</dt><dd>Synthetic type</dd></dl>
        <dl><dt>タイプ</dt><dd>{raw}</dd></dl>
        <dl><dt>レアリティ</dt><dd>Synthetic rarity</dd></dl></div>
        <div class="status">
        <span class="status-Item status-Item-Cost"><span class="heading">Cost</span>1</span>
        <span class="status-Item status-Item-Power"><span class="heading">Power</span>2</span>
        <span class="status-Item status-Item-Hp"><span class="heading">HP</span>3</span>
        </div><div class="detail">Synthetic effect</div></div>"""
        for name in ("Synthetic front", "Synthetic back")
    ]
    return (
        '<html><div class="cardlist-Detail">' + "".join(faces) + "</div></html>"
    ).encode()


@pytest.fixture(scope="module", params=CASES, ids=lambda case: case.raw)
def extracted(request: pytest.FixtureRequest) -> tuple[CardRecord, TraitCase]:
    case = cast("TraitCase", request.param)
    return extract_card(page(case.raw), number="SYN-001"), case


def expected_card(case: TraitCase) -> Card:
    return Card(
        number="SYN-001",
        faces=[
            Face(
                name=name,
                card_class="Synthetic class",
                card_type="Synthetic type",
                traits=list(case.complete),
                cost="1",
                power="2",
                hp="3",
                text="Synthetic effect",
                image="/synthetic.png",
            )
            for name in ("Synthetic front", "Synthetic back")
        ],
    )


def test_all_faces_keep_complete_traits_and_exact_raw(
    extracted: tuple[CardRecord, TraitCase],
) -> None:
    record, case = extracted
    assert len(record.faces) == 2
    for face in record.faces:
        assert face.traits == list(case.complete)
        assert face.trait_raw == case.raw
        assert jp_face(face).traits == case.complete


def test_v2_projection_keeps_complete_traits_without_mutating_faces(
    extracted: tuple[CardRecord, TraitCase],
) -> None:
    record, case = extracted
    before = asdict(record)
    expected = expected_card(case)
    actual = jp_card(record)
    assert actual == expected
    assert observation(actual, "jp") == observation(expected, "jp")
    assert asdict(record) == before


def test_projection_uses_parsed_traits_instead_of_retokenizing_raw(
    extracted: tuple[CardRecord, TraitCase],
) -> None:
    record, case = extracted
    changed = replace(
        record,
        faces=[replace(face, traits=["Separate value"]) for face in record.faces],
    )
    expected = expected_card(case)
    for face in expected.faces:
        face.traits = ["Separate value"]
    assert jp_card(changed) == expected
    assert all(face.traits == ["Separate value"] for face in changed.faces)


@pytest.mark.parametrize(
    "raw",
    [
        "Alpha・・〈Synthetic・Compound〉",
        "・〈Synthetic・Compound〉",
        "〈Synthetic・Compound〉・",
    ],
)
def test_outer_malformed_separators_still_fail(raw: str) -> None:
    with pytest.raises(ValidationError, match="malformed trait list"):
        extract_card(page(raw), number="SYN-001")
