"""Synthetic frozen evidence counterexamples; official names are never fixtures."""

import re
import shutil
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

import sve_carddb.domains.digital.name_policies.evaluate as module
from sve_carddb.core.json import array, canonical, digest, object_value
from sve_carddb.domains.digital.links.catalogue import complete_inventory
from sve_carddb.domains.digital.links.evidence import Evidence
from sve_carddb.domains.digital.links.importer import review_context
from sve_carddb.domains.digital.name_policies.catalogue import Catalogue, link_catalogue
from sve_carddb.domains.digital.name_policies.catalogue import (
    catalogue as current_catalogue,
)
from sve_carddb.domains.digital.name_policies.evaluate import (
    LINK_GAMES,
    OwnerEvidence,
    name_result,
    owner_text,
    rule_links,
)
from sve_carddb.domains.digital.name_policies.records import LinkPolicy

from ...support.digital_link_import_fixtures import catalogue_fixture
from ...support.digital_name_policy_fixtures import (
    PolicyFixture,
    copied_policy,
    make_policy_fixture,
)

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.core.provenance import SourceUse
    from sve_carddb.domains.digital.name_policies.evaluate import NameOwner


@pytest.fixture(scope="module")
def baseline(tmp_path_factory: pytest.TempPathFactory) -> PolicyFixture:
    return make_policy_fixture(tmp_path_factory.mktemp("name-policy-evaluate"))


@pytest.fixture(scope="module")
def catalogues(baseline: PolicyFixture) -> tuple[Catalogue, Catalogue]:
    snapshot = baseline.snapshot()
    assert snapshot.links is not None
    return current_catalogue(
        snapshot.current_names[0], baseline.digital.sources()
    ), link_catalogue(snapshot.links, baseline.digital.sources())


def synthetic(
    owner: NameOwner, text: str | None, uses: tuple[SourceUse, ...] = ()
) -> OwnerEvidence:
    return OwnerEvidence(owner, text, uses, digest(b"synthetic checked build context"))


def observed(baseline: PolicyFixture) -> OwnerEvidence:
    sources = baseline.digital.sources()
    return owner_text(baseline.owner(), sources, review_context(sources))


def test_full_catalogue_qualifies_first_game_and_plans_all_targets(
    baseline: PolicyFixture, catalogues: tuple[Catalogue, Catalogue]
) -> None:
    names, links = catalogues
    evidence = observed(baseline)
    result = name_result(evidence, names)
    assert result.status == "eligible"
    assert result.game == "sv1"
    assert result.text == "合成測試名"
    plans = rule_links((evidence, evidence), links)
    assert [(p.game, p.official_id) for p in plans] == [
        ("sv1", "123456789"),
        ("svwb", "22345678"),
    ]
    assert tuple(p.game for p in plans) == LINK_GAMES
    assert (
        len(plans[0].refs) == 2
    )  # normal/evolved share the same real name source, not fake phases
    assert all(
        p.subject()["face_id"] is None and p.subject()["digital_phase"] is None
        for p in plans
    )
    assert all(len(p.owners) == 1 for p in plans)
    assert not hasattr(result, "relation")
    assert not hasattr(plans[0], "text")


@pytest.mark.parametrize(
    "missing", ["null", "empty", "whitespace", "kana", "phase", "language", "different"]
)
def test_first_game_failure_never_borrows_second_game(
    baseline: PolicyFixture,
    catalogues: tuple[Catalogue, Catalogue],
    missing: str,
) -> None:
    names, links = catalogues
    changed = list(names.names)
    indices = [
        i for i, n in enumerate(changed) if n.game == "sv1" and n.lang == "zh-Hant"
    ]
    if missing in {"phase", "language"}:
        changed = [
            n
            for n in changed
            if not (
                n.game == "sv1"
                and n.lang == "zh-Hant"
                and (missing == "language" or n.phase == "evolved")
            )
        ]
    elif missing == "different":
        for n in names.names:
            if n.game == "sv1":
                changed.append(
                    replace(
                        n,
                        official_id="123456788",
                        text="第二個不同譯名" if n.lang == "zh-Hant" else n.text,
                    )
                )
    else:
        value = {
            "null": "",
            "empty": "",
            "whitespace": "\u2000\u3000",
            "kana": "仮テスト",
        }[missing]
        changed[indices[0]] = replace(changed[indices[0]], text=value)
    result = name_result(
        synthetic(baseline.owner(), "Synthetic card"),
        replace(names, names=tuple(changed)),
    )
    assert result.status == "untranslated"
    assert result.condition == "nonunique_or_missing_translation"
    assert result.text is None
    assert result.game is None
    assert len(rule_links((synthetic(baseline.owner(), "Synthetic card"),), links)) == 2


def test_only_second_game_and_exact_cjk_are_eligible(
    baseline: PolicyFixture, catalogues: tuple[Catalogue, Catalogue]
) -> None:
    names, _ = catalogues
    second_only = replace(
        names, names=tuple(n for n in names.names if n.game == "svwb")
    )
    assert (
        name_result(synthetic(baseline.owner(), "Synthetic card"), second_only).game
        == "svwb"
    )
    cjk = replace(names, names=tuple(replace(n, text="漢字") for n in names.names))
    result = name_result(synthetic(baseline.owner(), "漢字"), cjk)
    assert result.status == "eligible"
    assert result.text == "漢字"


@pytest.mark.parametrize(
    "text",
    [
        " Synthetic card",
        "Synthetic card ",
        "synthetic card",
        "Ｓynthetic card",
        "Synthetic\u200b card",
    ],
)
def test_no_normalization_or_trim(
    baseline: PolicyFixture, catalogues: tuple[Catalogue, Catalogue], text: str
) -> None:
    names, links = catalogues
    assert (
        name_result(synthetic(baseline.owner(), text), names).status == "untranslated"
    )
    assert rule_links((synthetic(baseline.owner(), text),), links) == ()


def test_hash_collision_does_not_establish_name_equality(
    baseline: PolicyFixture,
    catalogues: tuple[Catalogue, Catalogue],
    monkeypatch: pytest.MonkeyPatch,
) -> None:

    monkeypatch.setattr(module, "digest", lambda _: "sha256:" + "0" * 64)
    names, links = (replace(c, names=c.names) for c in catalogues)
    assert (
        name_result(
            synthetic(baseline.owner(), "Different synthetic name"), names
        ).status
        == "untranslated"
    )
    assert (
        rule_links((synthetic(baseline.owner(), "Different synthetic name"),), links)
        == ()
    )


def test_independent_exclusions_and_multiple_name_support(
    baseline: PolicyFixture, catalogues: tuple[Catalogue, Catalogue]
) -> None:
    names, links = catalogues
    owner = baseline.owner()
    name_hash = digest(b"Synthetic card")
    excluded_names = replace(names, excluded_names=frozenset({name_hash}))
    assert (
        name_result(synthetic(owner, "Synthetic card"), excluded_names).status
        == "excluded"
    )
    assert len(rule_links((synthetic(owner, "Synthetic card"),), links)) == 2
    excluded_links = replace(links, excluded_names=frozenset({name_hash}))
    assert rule_links((synthetic(owner, "Synthetic card"),), excluded_links) == ()
    second_evidence = tuple(
        replace(n, text="Other synthetic name") for n in links.names if n.lang == "ja"
    )
    supported = replace(excluded_links, names=(*links.names, *second_evidence))
    assert (
        len(
            rule_links(
                (
                    synthetic(owner, "Synthetic card"),
                    synthetic(owner, "Other synthetic name"),
                ),
                supported,
            )
        )
        == 2
    )
    blocked = replace(
        links, excluded_targets=frozenset({(owner.card_id, "sv1", "123456789")})
    )
    assert [
        p.game for p in rule_links((synthetic(owner, "Synthetic card"),), blocked)
    ] == ["svwb"]


def test_typed_purposes_cannot_be_interchanged(
    baseline: PolicyFixture, catalogues: tuple[Catalogue, Catalogue]
) -> None:
    names, links = catalogues
    with pytest.raises(
        ValueError, match=r"^Browsing policy cannot supply an official name$"
    ):
        name_result(synthetic(baseline.owner(), "Synthetic card"), links)
    with pytest.raises(
        ValueError, match=r"^Name policy cannot authorize browsing links$"
    ):
        rule_links((synthetic(baseline.owner(), "Synthetic card"),), names)


def test_unknown_printing_never_borrows_current(
    baseline: PolicyFixture, catalogues: tuple[Catalogue, Catalogue]
) -> None:
    owner = baseline.owner(state="unknown").model_copy(update={"kind": "printing_face"})
    sources = baseline.digital.sources()
    evidence = owner_text(owner, sources, review_context(sources))
    text, uses = evidence.text, evidence.uses
    assert text is None
    assert uses == ()
    assert name_result(evidence, catalogues[0]).status == "unknown"
    assert rule_links((evidence,), catalogues[1]) == ()


@pytest.mark.parametrize(
    ("fault", "message"),
    [
        ("context", "Digital-name owner resolver differs from build context"),
        ("card", "Digital-name owner identity or parent card mismatch"),
        ("face", "Digital-name owner identity or parent card mismatch"),
        ("printing", "Digital-name owner identity or parent card mismatch"),
        ("batch", "Digital-name owner source is outside build closure"),
        ("hash", "Evidence must locate exact hash-verified text"),
        ("field", "Digital-name owner printing face source mismatch"),
        ("observation", "Digital-name owner registry observation mismatch"),
    ],
)
def test_owner_counterexamples_are_exact(
    baseline: PolicyFixture, fault: str, message: str
) -> None:
    owner = baseline.owner()
    sources = baseline.digital.sources()
    review = review_context(sources)
    if fault == "context":
        review = review.model_copy(
            update={
                "context": review.context.model_copy(update={"configuration": "{}"})
            }
        )
    elif fault in {"card", "face", "printing"}:
        owner = owner.model_copy(
            update={
                fault + "_id": {"card": "c:", "face": "f:", "printing": "p:"}[fault]
                + "0" * 32
            }
        )
    elif fault in {"batch", "hash", "field"}:
        ref = owner.name_ref
        assert ref is not None
        if fault == "batch":
            ref = ref.model_copy(update={"batch_id": digest(b"wrong batch")})
        elif fault == "hash":
            ref = ref.model_copy(update={"text_hash": digest(b"wrong name")})
        else:
            lang, document, source = sources.document(ref)
            faces = array(object_value(document)["faces"])
            object_value(faces[0])["card_type"] = "Synthetic card"
            ref = ref.model_copy(update={"locator": "/faces/0/card_type"})
            sources.cache[ref.batch_id, ref.source_version_id, ref.parser] = (
                lang,
                document,
                source,
            )
        owner = owner.model_copy(update={"name_ref": ref})
    else:
        original = Evidence(sources).index(review)
        printing = original.printings[owner.printing_id]
        altered = printing.model_copy(
            update={
                "observation": printing.observation.model_copy(
                    update={"rules_hash": digest(b"wrong rules")}
                )
            }
        )
        sources.identity_indexes[canonical(review.context.model_dump(mode="json"))] = (
            replace(original, printings={**original.printings, altered.id: altered})
        )
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        owner_text(owner, sources, review)


@pytest.mark.parametrize("parent", ["face", "printing"])
def test_unknown_owner_still_requires_each_parent_card(
    baseline: PolicyFixture, parent: str
) -> None:
    owner = baseline.owner(state="unknown")
    sources = baseline.digital.sources()
    review = review_context(sources)
    original = Evidence(sources).index(review)
    other_card = "c:" + "0" * 32
    if parent == "face":
        face = original.faces[owner.face_id].model_copy(update={"card_id": other_card})
        altered = replace(original, faces={**original.faces, face.id: face})
    else:
        printing = original.printings[owner.printing_id].model_copy(
            update={"card_id": other_card}
        )
        altered = replace(
            original, printings={**original.printings, printing.id: printing}
        )
    sources.identity_indexes[canonical(review.context.model_dump(mode="json"))] = (
        altered
    )
    with pytest.raises(
        ValueError, match=r"^Digital-name owner identity or parent card mismatch$"
    ):
        owner_text(owner, sources, review)


def test_entire_null_hant_page_still_blocks_qualification(
    baseline: PolicyFixture, tmp_path: Path
) -> None:
    copied = copied_policy(baseline, tmp_path / "repo")

    shutil.move(copied.root / "catalogue-store", copied.root / "previous-store")
    copied = replace(
        copied,
        digital=replace(copied.digital, store=copied.root / "previous-store/archive"),
    )

    def change(data: dict[str, JsonValue], language: str) -> None:
        if language == "zh-tw":
            for item in array(data["cards"]):
                object_value(item)["card_name"] = None

    digital = catalogue_fixture(
        copied.digital, game="sv1", languages=("ja", "zh-tw"), transform=change
    )
    sources = digital.sources()

    names = complete_inventory(sources, review_context(sources), "sv1")
    assert not names["sv1", "123456789", "normal", "zh-Hant"].text


def test_rule_plans_reject_cross_build_owners(
    baseline: PolicyFixture, catalogues: tuple[Catalogue, Catalogue]
) -> None:
    evidence = synthetic(baseline.owner(), "Synthetic card")
    with pytest.raises(
        ValueError,
        match=r"^Digital-name link owners belong to different build contexts$",
    ):
        rule_links(
            (evidence, replace(evidence, context_hash=digest(b"different context"))),
            catalogues[1],
        )


def test_unknown_owner_with_borrowed_reference_is_rejected(
    baseline: PolicyFixture,
) -> None:
    from sve_carddb.domains.digital.name_policies.evaluate import NameOwner  # ruff: ignore[import-outside-top-level] -- model validation requires the runtime type
    from sve_carddb.domains.digital.name_policies.loader import model  # ruff: ignore[import-outside-top-level] -- test enters the sanitized model boundary

    raw = baseline.owner().model_dump(mode="json")
    raw["state"] = "unknown"
    with pytest.raises(ValueError, match=r"^Invalid digital-name policy fields$"):
        model(NameOwner, raw)


@pytest.mark.parametrize("fault", ["name", "target", "parent"])
def test_catalogue_refusals_are_reachable(baseline: PolicyFixture, fault: str) -> None:
    from sve_carddb.domains.digital.name_policies.evaluate import _parents  # ruff: ignore[import-outside-top-level] -- checks the catalogue parent boundary directly

    links = baseline.snapshot().links
    assert links is not None
    sources = baseline.digital.sources()
    if fault == "parent":
        full = complete_inventory(sources, review_context(sources), "sv1")
        name = next(iter(full.values()))
        name.common["base_card_id"] = 999999999
        with pytest.raises(
            ValueError, match=r"^Digital-name catalogue parent closure mismatch$"
        ):
            _parents(full, "sv1")
        return
    document = links.model_dump(mode="json")
    content = object_value(document["content"])
    if fault == "name":
        content["excluded_names"] = [
            {
                "source_lang": "ja",
                "source_name_hash": digest(b"absent name"),
                "reason": "Synthetic missing name",
            }
        ]
        message = "Digital-name exclusion cannot locate its frozen Japanese name"
    else:
        content["excluded_targets"] = [
            {
                "card_id": "c:" + "0" * 32,
                "game": "sv1",
                "official_id": "123456789",
                "reason": "Synthetic missing card",
            }
        ]
        message = "Digital-name exclusion cannot locate its card target"
    broken = LinkPolicy.model_validate_json(canonical(document))
    with pytest.raises(ValueError, match="^" + message + "$"):
        link_catalogue(broken, sources)


def test_catalogue_rejects_duplicate_id_even_when_rows_equal(
    baseline: PolicyFixture,
) -> None:
    sources = baseline.digital.sources()
    complete_inventory(sources, review_context(sources), "sv1")
    for key, (_, document, _) in sources.cache.items():
        if key[2] == "translation-sv1-v1":
            cards = array(object_value(object_value(document)["data"])["cards"])
            cards.append(cards[0])
            break
    with pytest.raises(
        ValueError, match=r"^Digital catalogue repeats an ID within a page$"
    ):
        complete_inventory(sources, review_context(sources), "sv1")
