"""Each closed draw variant admits only its exact source head and body."""

import hashlib
import json
from pathlib import Path

import pytest

from sve_carddb.contracts.author_evidence import SourceUse
from sve_carddb.contracts.author_leaves import ParamDecl, ParamSource
from sve_carddb.domains.effects.author_sources import entry_payload, parameter_expected
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from sve_carddb.domains.translations.four_layer_semantics import CardContext
from tests.support.author_source_fixtures import SourceCandidate, source_context

from ..translations.test_four_layer_classification import classifier, source


def frozen_context(card_no: str, checksum: str) -> tuple[str, CardContext]:
    root = (
        Path(__file__).resolve().parents[4]
        / "card-data"
        / "jp"
        / card_no.split("-", 1)[0]
    )
    faces = [
        face
        for path in root.rglob("[0-9][0-9][0-9]-[0-9][0-9][0-9].json")
        for card in json.loads(path.read_bytes())["cards"]
        if card["card_no"] == card_no
        for document in card["documents"].values()
        for face in document["record"]["faces"]
        if hashlib.sha256(face["text"].encode()).hexdigest() == checksum
    ]
    found = bool(faces)
    assert found
    raw = faces[0]["text"]
    facts = {(face["card_type"], len(face.get("sections", []))) for face in faces}
    assert len(facts) == 1
    card_type, sections = facts.pop()
    phase = (
        "evolved"
        if "エボルヴ" in card_type
        else "advance"
        if "アドバンス" in card_type
        else "normal"
    )
    kind = "spell" if "スペル" in card_type else "follower"
    return raw, CardContext(
        source(raw), "c:synthetic", "f:synthetic", phase, kind, True, sections
    )


@pytest.mark.parametrize(
    ("card_no", "checksum", "line", "variant", "count"),
    [
        (
            "BP02-053",
            "042d2d10c6f1a140b01524f8d82c67852348965ec0626cd1a287acfe9dc7edc7",
            0,
            "draw_exact_evolve_time.v1",
            3,
        ),
        (
            "BP03-006",
            "581c88d5afb45f02ecc30ffc936194bdd22f4587498cd00f71d26e5de186a6f4",
            1,
            "draw_exact_evolve_time.v1",
            1,
        ),
        (
            "BP20-022",
            "c5ac833a9c464592e239c54b8761683ef0155a400ec6736d79594968d75f1f41",
            2,
            "draw_exact_attack_time.v1",
            1,
        ),
    ],
)
def test_frozen_draw_sources_admit_their_complete_ability(
    card_no: str, checksum: str, line: int, variant: str, count: int
) -> None:
    raw, context = frozen_context(card_no, checksum)
    descriptor = context.source
    engine = classifier()
    candidate = SourceCandidate(raw, descriptor, engine, context)
    derived = derive_source(raw, descriptor, engine, context=context)
    index = next(
        i for i, part in enumerate(derived.field.parts) if part.line_ordinal == line
    )
    frame, binding = derived.bind(index)
    assert frame.semantic_variant.key == variant
    assert binding.values == {"leaf_0": count}
    assert frame.projection.imports == frame.projection.exports == ()
    use = SourceUse(
        binding_id=binding.id,
        occurrence=binding.occurrence_key(),
        ability_scope="ability_0",
    )
    checked = source_context(candidate, use)
    # Fixture contexts carry no source admission proof.
    assert checked.source_proof == "none"
    declaration = ParamDecl(
        type="Nat", source=ParamSource(slot="leaf_0", component="value")
    )
    name = {
        "draw_exact_spell.v1": "spell",
        "draw_exact_attack_time.v1": "attack_time",
        "draw_exact_evolve_time.v1": "evolve_time",
    }[variant]
    assert entry_payload(checked, "ability_0") == {
        name: {"do": [{"draw": {"who": "self", "n": count}}]},
        "ports": {"imports": {}, "exports": {}},
    }
    assert (
        parameter_expected(
            checked, declaration, "A.act.draw", ("n",), {"n": {"param": "n"}}
        )
        == count
    )
