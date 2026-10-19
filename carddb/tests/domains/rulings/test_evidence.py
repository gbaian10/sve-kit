"""Partial historical evidence must never activate the known subset of a ruling."""

from dataclasses import replace

import pytest

from sve_carddb.core.json import digest
from sve_carddb.domains.effects.classifications import Basis, Member
from sve_carddb.domains.effects.reader import Catalog
from sve_carddb.domains.rulings.evidence import Missing, capture
from sve_carddb.domains.rulings.mapping import LegacyUse, Namespace, Rebuild
from sve_carddb.domains.rulings.resolution import build

from ..effects.test_classifications import classification, member
from .test_mapping import SyntheticSources
from .test_rulings import ruling


def frozen() -> Catalog:
    first = member()
    second = first.model_copy(
        update={
            "id": digest(b"second"),
            "position": first.position.model_copy(update={"card_no": "TEST-002"}),
        }
    )
    return Catalog(
        "fixture",
        Basis(recipe="fixture-v1", inputs_hash=digest(b"inputs")),
        (classification().model_copy(update={"members": (first.id, second.id)}),),
        (first, second),
    )


def verify_catalog(catalog: Catalog) -> Namespace:
    if catalog != frozen():
        raise ValueError("Fixture differs from its frozen complete input")
    return Namespace(
        code="fixture", id_recipe="fixture-t-v1", normalizer_version="fixture-v1"
    )


def test_missing_member_keeps_entire_reference_pending() -> None:
    sources = SyntheticSources()
    first = sources.use("a")

    def resolve(item: Member) -> LegacyUse | Missing:
        if item.id != member().id:
            return Missing(member_id=item.id, reason="unproven_role")
        return LegacyUse(
            occurrence=first.target().occurrence, scope=first.scope, parameters={}
        )

    proof = capture(
        frozen(),
        frozenset({"T-example"}),
        verify_catalog=verify_catalog,
        resolve_member=resolve,
    )
    rebuild = Rebuild((proof.inputs,), (first,), sources.verify)
    report = build((ruling(applies_to=["T-example"]),), rebuild)
    assert report.payload()["reference_pending"] == 1
    assert report.payload()["active_template_edges"] == 0
    assert not report.mappings
    assert report.resolutions[0].legacy.occurrence is None
    assert report.resolutions[0].legacy.namespace == "fixture"
    assert report.resolutions[0].candidates == ()
    assert proof.missing["T-example"][0].reason == "unproven_role"


def test_complete_printing_members_deduplicate_exact_source_uses() -> None:
    sources = SyntheticSources()
    old = sources.use("a")
    other = sources.use("b")
    use = LegacyUse(occurrence=old.target().occurrence, scope=old.scope, parameters={})
    proof = capture(
        frozen(),
        frozenset({"T-example"}),
        verify_catalog=verify_catalog,
        resolve_member=lambda _: use,
    )
    rebuild = Rebuild((proof.inputs,), (old, other), sources.verify)
    report = build((ruling(applies_to=["T-example", "T-example"]),), rebuild)
    assert len(report.resolutions) == 2
    assert {r.ruling_ref.reference_ordinal for r in report.resolutions} == {0, 1}
    assert all(r.target == old.target() for r in report.resolutions)
    assert report.applicable(other.target().occurrence) == ()
    assert proof.missing["T-example"] == ()


def test_complete_old_use_without_current_target_is_not_lost() -> None:
    sources = SyntheticSources()
    first, second = sources.use("a"), sources.use("b")
    uses = {
        m.id: LegacyUse(occurrence=u.target().occurrence, scope=u.scope, parameters={})
        for m, u in zip(frozen().members, (first, second), strict=True)
    }
    proof = capture(
        frozen(),
        frozenset({"T-example"}),
        verify_catalog=verify_catalog,
        resolve_member=lambda m: uses[m.id],
    )
    report = build(
        (ruling(applies_to=["T-example"]),),
        Rebuild((proof.inputs,), (first,), sources.verify),
    )
    assert {r.legacy.occurrence for r in report.resolutions} == {
        first.target().occurrence,
        second.target().occurrence,
    }
    assert sorted(r.status for r in report.resolutions) == ["pending", "resolved"]
    assert (
        next(r for r in report.resolutions if r.status == "pending").reason
        == "no_candidate"
    )


def test_missing_or_changed_frozen_catalog_is_rejected() -> None:
    with pytest.raises(ValueError, match="frozen complete input"):
        capture(
            replace(frozen(), members=frozen().members[:1]),
            frozenset({"T-example"}),
            verify_catalog=verify_catalog,
            resolve_member=lambda m: Missing(member_id=m.id, reason="missing_source"),
        )


def test_declared_bad_source_is_error_not_pending() -> None:
    def resolve(_: Member) -> LegacyUse:
        raise ValueError("Source owner hash mismatch")

    with pytest.raises(ValueError, match="Source owner hash mismatch"):
        capture(
            frozen(),
            frozenset({"T-example"}),
            verify_catalog=verify_catalog,
            resolve_member=resolve,
        )


def test_evidence_cannot_substitute_another_members_gap() -> None:
    with pytest.raises(ValueError, match="another member"):
        capture(
            frozen(),
            frozenset({"T-example"}),
            verify_catalog=verify_catalog,
            resolve_member=lambda _: Missing(
                member_id=digest(b"absent"), reason="missing_source"
            ),
        )


def test_namespace_mismatch_is_rejected() -> None:
    with pytest.raises(ValueError, match="namespace"):
        capture(
            frozen(),
            frozenset({"T-example"}),
            verify_catalog=lambda _: Namespace(
                code="other", id_recipe="r", normalizer_version="n"
            ),
            resolve_member=lambda m: Missing(member_id=m.id, reason="missing_source"),
        )


def test_same_occurrence_cannot_have_two_scopes_or_parameters() -> None:
    sources = SyntheticSources()
    current = sources.use("a")

    def resolve(m: Member) -> LegacyUse:
        return LegacyUse(
            occurrence=current.target().occurrence,
            scope=current.scope,
            parameters={"offset": 1 if m.id == member().id else 2},
        )

    with pytest.raises(ValueError, match="Printings disagree"):
        capture(
            frozen(),
            frozenset({"T-example"}),
            verify_catalog=verify_catalog,
            resolve_member=resolve,
        )
