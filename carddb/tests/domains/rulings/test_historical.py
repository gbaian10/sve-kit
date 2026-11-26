"""Authored domains cannot borrow or duplicate historical source uses."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.contracts.rulings import HistoricalUse, LegacyReference, Scope
from sve_carddb.core.json import canonical, digest
from sve_carddb.domains.rulings.historical import (
    Observation,
    inputs,
    references,
    verify_use,
)
from sve_carddb.domains.rulings.mapping import Rebuild
from sve_carddb.domains.rulings.resolution import build

from .test_mapping import SyntheticSources
from .test_rulings import ruling

if TYPE_CHECKING:
    from pydantic import JsonValue


def reference() -> LegacyReference:
    source = SyntheticSources().use(raw="synthetic")
    return LegacyReference(
        legacy="T0123456789",
        namespace="classification-v0",
        uses=(
            HistoricalUse(
                cards=("TEST-001", "TEST-002"), occurrence=source.target().occurrence
            ),
        ),
    )


def observation(_: HistoricalUse) -> Observation:
    return Observation("synthetic", frozenset({"TEST-001", "TEST-002"}))


def test_authored_scope_is_verified_before_resolution_and_reruns_after_rekey() -> None:
    ref = reference()
    doc = ruling(
        applies_to=[
            ref.model_dump(mode="json"),
            "E.synthetic",
            ref.model_dump(mode="json"),
        ]
    )
    with pytest.raises(ValueError, match="verified build sources"):
        build((doc,))
    old = inputs((doc,))
    missing = build((doc,), Rebuild((old,), (), lambda _: None))
    assert [r.ruling_ref.reference_ordinal for r in missing.resolutions] == [0, 2]
    assert all(r.reason == "no_candidate" for r in missing.resolutions)
    sources = SyntheticSources()
    for variant in (None, "synthetic.one", "synthetic.two"):
        use = sources.use(raw="synthetic", variant=variant)
        use = replace(use, scope=Scope(role="body", domain="effect"))
        sources.uses[canonical(use.target().model_dump(mode="json"))] = use
        report = build((doc,), Rebuild((old,), (use,), sources.verify))
        assert report.retained[0].ruling_ref.reference_ordinal == 1
        assert all(
            r.target == (None if variant is None else use.target())
            for r in report.resolutions
        )
        assert all(
            r.status == ("pending" if variant is None else "resolved")
            for r in report.resolutions
        )
    with pytest.raises(ValueError, match="lacks verified definition"):
        build((doc,), Rebuild((), (), lambda _: None))


def test_same_template_cannot_claim_conflicting_domains() -> None:
    ref = reference()
    first = ruling(applies_to=[ref.model_dump(mode="json")])
    narrowed = ref.model_copy(
        update={"uses": (ref.uses[0].model_copy(update={"cards": ("TEST-001",)}),)}
    )
    second = ruling(id="R-0002", applies_to=[narrowed.model_dump(mode="json")])
    with pytest.raises(ValueError, match="Conflicting"):
        references((first, second))


@pytest.mark.parametrize("case", ["hash", "printing", "line", "span"])
def test_false_source_facts_fail_instead_of_becoming_pending(case: str) -> None:
    use = reference().uses[0]
    if case == "hash":
        observed = Observation("changed", observation(use).cards)
    elif case == "printing":
        observed = Observation("synthetic", frozenset({"TEST-001"}))
    else:
        occurrence = use.occurrence.model_dump(mode="json")
        if case == "line":
            occurrence["line_ordinal"] = 1
        else:
            occurrence["segments"] = [{"start": 0, "end": 10}]
        use = HistoricalUse.model_validate_json(
            canonical({"cards": list(use.cards), "occurrence": occurrence})
        )
        observed = observation(use)
    with pytest.raises(ValueError, match="Historical"):
        verify_use(use, observed)


@pytest.mark.parametrize(
    "change",
    [
        {"namespace": "other"},
        {"uses": []},
        {"legacy": "E.synthetic"},
        {"uses": [{"cards": [], "occurrence": {}}]},
    ],
)
def test_reference_shape_rejects_unknown_or_empty_domains(
    change: dict[str, JsonValue],
) -> None:
    with pytest.raises(ValueError, match="Invalid versioned ruling"):
        ruling(applies_to=[reference().model_dump(mode="json") | change])


@pytest.mark.parametrize("reverse", [False, True])
def test_bare_reference_cannot_borrow_another_rulings_explicit_domain(
    reverse: bool,
) -> None:
    ref = reference()
    docs = (
        ruling(applies_to=[ref.model_dump(mode="json")]),
        ruling(id="R-0002", applies_to=[ref.legacy]),
    )
    with pytest.raises(ValueError, match="Mixed bare and explicit"):
        inputs(tuple(reversed(docs)) if reverse else docs)
    report = build((docs[1],))
    assert report.resolutions[0].level == "reference"
    assert report.resolutions[0].legacy.namespace is None


@pytest.mark.parametrize(
    "separator",
    ["\r", "\x0b", "\x0c"],
)
def test_only_lf_advances_historical_line_numbers(separator: str) -> None:
    raw = "prefix" + separator + "body\nlast"
    use = reference().uses[0]
    occurrence = use.occurrence.model_copy(
        update={
            "source_hash": digest(raw.encode())[7:],
            "segments": (
                use.occurrence.segments[0].model_copy(update={"start": 7, "end": 11}),
            ),
        }
    )
    use = use.model_copy(update={"occurrence": occurrence})
    verify_use(use, Observation(raw, frozenset(use.cards)))
    last = use.model_copy(
        update={
            "occurrence": occurrence.model_copy(
                update={
                    "line_ordinal": 1,
                    "segments": (
                        occurrence.segments[0].model_copy(
                            update={"start": 12, "end": 16}
                        ),
                    ),
                }
            )
        }
    )
    verify_use(last, Observation(raw, frozenset(use.cards)))


def test_historical_arrays_require_canonical_order() -> None:
    ref = reference()
    use = ref.uses[0]
    with pytest.raises(ValueError, match="printings must be sorted"):
        HistoricalUse(cards=tuple(reversed(use.cards)), occurrence=use.occurrence)
    other = HistoricalUse(
        cards=use.cards,
        occurrence=SyntheticSources().use("z", raw="synthetic").target().occurrence,
    )
    ordered = tuple(
        sorted(
            (use, other), key=lambda u: canonical(u.occurrence.model_dump(mode="json"))
        )
    )
    LegacyReference(legacy=ref.legacy, namespace=ref.namespace, uses=ordered)
    with pytest.raises(ValueError, match="sorted by canonical occurrence"):
        LegacyReference(
            legacy=ref.legacy, namespace=ref.namespace, uses=tuple(reversed(ordered))
        )
