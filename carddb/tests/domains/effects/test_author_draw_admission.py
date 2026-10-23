"""Resolved draw sources must not borrow the unrelated author entry adapter."""

import hashlib
import json
from pathlib import Path

import pytest

from sve_carddb.contracts.author_evidence import SourceUse
from sve_carddb.contracts.author_leaves import ParamDecl, ParamSource
from sve_carddb.domains.effects.author_sources import (
    SourceCandidate,
    SourceError,
    entry_payload,
    parameter_expected,
    resolve_source,
)
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from sve_carddb.domains.translations.four_layer_semantics import CardContext

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
        for path in root.glob("[0-9][0-9][0-9]-[0-9][0-9][0-9].json")
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
    phase = "evolved" if "エボルヴ" in card_type else "normal"
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
            "BP03-050",
            "1ed4c4e8e1c709c0af61cb8f42212e10759728f264197898321166f8e5fa5c47",
            0,
            "draw_exact_evolve_time.v1",
            1,
        ),
        (
            "BP05-095",
            "5e87ebd2c59b30553d04f4a9c43cc350457fd60d7ea7ae8571c11d3a6d640762",
            1,
            "draw_exact_evolve_time.v1",
            2,
        ),
        (
            "BP05-111",
            "dddfee5894105c6af4c5e6f9db8bc3741a028e4110fd9eaf55818bfc9ce7ee29",
            1,
            "draw_exact_evolve_time.v1",
            1,
        ),
        (
            "BP10-122",
            "ace9cac1e1f1feca911aab489957cbc51980ec818163a67f47690cd6a0745ff8",
            0,
            "draw_exact_attack_time.v1",
            1,
        ),
        (
            "BP14-117",
            "1ed4c4e8e1c709c0af61cb8f42212e10759728f264197898321166f8e5fa5c47",
            0,
            "draw_exact_evolve_time.v1",
            1,
        ),
        (
            "BP15-107",
            "dddfee5894105c6af4c5e6f9db8bc3741a028e4110fd9eaf55818bfc9ce7ee29",
            1,
            "draw_exact_evolve_time.v1",
            1,
        ),
        (
            "BP15-120",
            "17ee2b74e4b5d4e1d2c98c5cd3a6ab6f0049e6d02b921ae97b66f1ffb1498f64",
            0,
            "draw_exact_evolve_time.v1",
            2,
        ),
        (
            "BP17-111",
            "fdf8f822c9659658ca1bd41842eb78d605fbb8758485d818aae7175ea22f7719",
            1,
            "draw_exact_evolve_time.v1",
            1,
        ),
        (
            "BP19-032",
            "89e47afc4e86b7585d414ae12ee2accbb0e22ea5856f559510ecd46fcaf8b8ed",
            2,
            "draw_exact_attack_time.v1",
            1,
        ),
        (
            "BP19-033",
            "cab7ff01522cd03ff10ad39c00bea510c0aaabd2811384457233bbbb7a53737e",
            1,
            "draw_exact_attack_time.v1",
            1,
        ),
        (
            "BP20-022",
            "c5ac833a9c464592e239c54b8761683ef0155a400ec6736d79594968d75f1f41",
            2,
            "draw_exact_attack_time.v1",
            1,
        ),
        (
            "BP20-108",
            "f6600ffacbefbd1ac80cb51fd78a44345f06202e6d282a18709a642732286765",
            1,
            "draw_exact_attack_time.v1",
            1,
        ),
        (
            "BP21-028",
            "1ed4c4e8e1c709c0af61cb8f42212e10759728f264197898321166f8e5fa5c47",
            0,
            "draw_exact_evolve_time.v1",
            1,
        ),
        (
            "CP02-068",
            "43c6e64ac2131edc00347517e27a9e4a294c87caf49ce2055db12be7b3a32c28",
            0,
            "draw_exact_spell.v1",
            3,
        ),
        (
            "CP04-002",
            "ab133351901c37e66df9b5535a5c14a467e16c45eb97ddf064c8fcd617fdee6e",
            0,
            "draw_exact_evolve_time.v1",
            1,
        ),
        (
            "ECP01-002",
            "1ed4c4e8e1c709c0af61cb8f42212e10759728f264197898321166f8e5fa5c47",
            0,
            "draw_exact_evolve_time.v1",
            1,
        ),
    ],
)
def test_frozen_draw_sources_resolve_without_author_admission(
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
    checked = resolve_source(candidate, use)
    # Reconstructed wording with synthetic identities is not a current registry proof.
    assert checked.source_proof == "derived_self_consistent"
    with pytest.raises(SourceError, match="unsupported_source_ability_mapping"):
        entry_payload(checked, "ability_0")
    declaration = ParamDecl(
        type="Nat", source=ParamSource(slot="leaf_0", component="value")
    )
    with pytest.raises(SourceError, match="unsupported_source_construct"):
        parameter_expected(
            checked, declaration, "A.act.draw", ("n",), {"n": {"param": "n"}}
        )
