"""Explicit frozen synthetic domains exercise the public many-to-many rebuild boundary."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.contracts.four_layer import (
    FaceRevisionOwner,
    Frame,
    LeafSchema,
    Projection,
    SemanticVariant,
    Source,
    hash_payload,
)
from sve_carddb.contracts.rulings import RulingRef, Scope
from sve_carddb.contracts.source_binding import SourceBinding
from sve_carddb.core.json import canonical, digest
from sve_carddb.domains.rulings.mapping import (
    Definition,
    FrameUse,
    LegacyInputs,
    LegacyUse,
    Namespace,
    Rebuild,
    key,
)
from sve_carddb.domains.rulings.resolution import build
from sve_carddb.domains.rulings.storage import write
from sve_carddb.domains.translations.four_layer_normalizer import normalize_source

from ..translations.test_four_layer_normalizer import source
from ..translations.test_four_layer_storage import stored as unbound  # ruff: ignore[unused-import] -- pytest resolves the parent fixture by name
from .test_rulings import compiled as compiled  # ruff: ignore[useless-import-alias] -- register real SQLite fixtures
from .test_rulings import ruling
from .test_rulings import stored as stored  # ruff: ignore[useless-import-alias] -- register real ruling fixture

if TYPE_CHECKING:
    from sve_carddb.build import Database


class SyntheticSources:
    def __init__(self) -> None:
        self.uses: dict[bytes, FrameUse] = {}
        self.raw: dict[bytes, str] = {}

    def use(
        self,
        owner: str = "a",
        raw: str = "Synthetic sentence.",
        variant: str | None = "synthetic.body.a",
    ) -> FrameUse:
        descriptor = source(raw).model_copy(
            update={"owner": FaceRevisionOwner(kind="face_revision", revision_id=owner)}
        )
        field = normalize_source(raw, descriptor)
        part = field.parts[0]
        occurrence = LegacyUse.model_validate_json(
            canonical(
                {
                    "occurrence": {
                        **descriptor.model_dump(
                            mode="json",
                            include={"owner", "field", "ordinal", "source_hash"},
                        ),
                        "line_ordinal": part.line_ordinal,
                        "role": part.source_span.role,
                        "segments": [
                            s.model_dump(mode="json") for s in part.source_span.segments
                        ],
                    },
                    "scope": {"role": "body", "domain": "synthetic_effect"},
                    "parameters": {},
                }
            )
        ).occurrence
        frame = Frame(
            id="frame:" + "0" * 64,
            source=Source(
                source_lang="ja",
                canonical_hash=digest(part.canonical_source.encode())[7:],
                normalizer_version="synthetic-v1",
            ),
            role="body",
            semantic_variant=SemanticVariant(
                state="resolved" if variant else "pending",
                key=variant,
                scope=None if variant else occurrence,
            ),
            leaf_schema=LeafSchema(format=2, slots=()),
            projection=Projection(
                projection_kind="ability_body" if variant else "pending",
                discriminator=variant,
                scopes=(),
                imports=(),
                exports=(),
            ),
            content_hash="0" * 64,
        )
        checksum = hash_payload(frame.payload(part.canonical_source))
        frame = frame.model_copy(
            update={"id": "frame:" + checksum, "content_hash": checksum}
        )
        binding = SourceBinding(
            id="bind:" + "0" * 64,
            source=descriptor,
            ordinal=part.ordinal,
            line_ordinal=part.line_ordinal,
            frame_id=frame.id,
            source_span=part.source_span,
            values={},
            occurrences=(),
            trace=part.trace,
        )
        binding = binding.model_copy(
            update={"id": "bind:" + hash_payload(binding.payload())}
        )
        use = FrameUse(frame, binding, Scope(role="body", domain="synthetic_effect"))
        self.uses[key(use.target())] = use
        self.raw[key(descriptor)] = raw
        return use

    def verify(self, use: FrameUse) -> None:
        expected = self.uses.get(key(use.target()))
        if expected is None or expected != use:
            raise ValueError("Unknown or borrowed current source")
        raw = self.raw[key(use.binding.source)]
        use.binding.source.verify(expected.binding.source, raw)
        part = normalize_source(raw, use.binding.source).parts[use.binding.ordinal]
        use.frame.verify(part.canonical_source)
        use.binding.verify(use.frame, {})


def definition(identifier: str, *uses: FrameUse) -> Definition:
    return Definition(
        template_id=identifier,
        semantic_variant="synthetic.old",
        uses=tuple(
            LegacyUse(occurrence=u.target().occurrence, scope=u.scope, parameters={})
            for u in uses
        ),
    )


def legacy(
    *definitions: Definition,
    expected: tuple[Definition, ...] | None = None,
    namespace: str = "synthetic-old-v1",
) -> LegacyInputs:
    frozen = {d.template_id: d for d in (definitions if expected is None else expected)}

    def verify(item: Definition) -> None:
        if frozen.get(item.template_id) != item:
            raise ValueError("Definition or complete old source domain differs")

    return LegacyInputs(
        Namespace(
            code=namespace,
            id_recipe="synthetic-id-list-v1",
            normalizer_version="synthetic-v0",
        ),
        tuple(definitions),
        verify,
    )


def test_split_merge_and_repeated_references_preserve_every_exact_old_use() -> None:
    sources = SyntheticSources()
    a, b, extra = (
        sources.use("a"),
        sources.use("b", variant="synthetic.body.b"),
        sources.use("extra"),
    )
    a2 = sources.use("a2")
    first = definition("T0123456789", a, b)
    second = definition("T1111111111", a2)
    rebuild = Rebuild((legacy(first, second),), (a, b, a2, extra), sources.verify)
    report = build(
        (
            ruling(
                applies_to=[
                    first.template_id,
                    first.template_id,
                    second.template_id,
                    "E.synthetic",
                ]
            ),
        ),
        rebuild,
    )
    assert {
        (r.ruling_ref.reference_ordinal, r.legacy.occurrence)
        for r in report.resolutions
    } == {
        (0, a.target().occurrence),
        (0, b.target().occurrence),
        (1, a.target().occurrence),
        (1, b.target().occurrence),
        (2, a2.target().occurrence),
    }
    assert len(rebuild.mappings) == 3
    assert report.payload()["template_references"] == 3
    assert report.payload()["occurrence_resolved"] == 5
    assert report.payload()["retained_non_template"] == 1
    assert report.applicable(extra.target().occurrence) == ()
    assert (
        report.require(
            RulingRef(id="R-0001", revision=1, reference_ordinal=0),
            a.target().occurrence,
        ).target
        == a.target()
    )
    groups = report.payload()["frame_families"]
    assert isinstance(groups, list)
    merged = next(
        g for g in groups if isinstance(g, dict) and g["frame_id"] == a.frame.id
    )
    assert isinstance(merged, dict)
    members = merged["families"]
    assert isinstance(members, list)
    assert {m["legacy_template_id"] for m in members if isinstance(m, dict)} == {
        first.template_id,
        second.template_id,
    }


@pytest.mark.parametrize(
    ("case", "reason"),
    [
        ("missing", "no_candidate"),
        ("changed", "source_changed"),
        ("scope", "unsupported_relation"),
        ("pending", "unsupported_relation"),
        ("ambiguous", "ambiguous_variant"),
    ],
)
def test_known_domain_pending_never_enters_executable_applicability(
    case: str, reason: str
) -> None:
    sources = SyntheticSources()
    old = sources.use(variant=None if case == "pending" else "synthetic.body.a")
    current: tuple[FrameUse, ...] = (old,)
    if case == "missing":
        current = ()
    elif case == "changed":
        current = (sources.use(raw="Changed source."),)
    elif case == "scope":
        widened = replace(old, scope=Scope(role="body", domain="wider_domain"))
        sources.uses[key(widened.target())] = widened
        current = (widened,)
    elif case == "ambiguous":
        current = (old, sources.use(variant="synthetic.body.b"))
    rebuild = Rebuild(
        (legacy(definition("T0123456789", old)),), current, sources.verify
    )
    report = build((ruling(applies_to=["T0123456789"]),), rebuild)
    selected = report.resolutions[0]
    assert selected.reason == reason
    assert selected.level == "occurrence"
    assert selected.target is None
    assert report.applicable(old.target().occurrence) == ()
    with pytest.raises(ValueError, match="executable closure"):
        report.require(selected.ruling_ref, old.target().occurrence)
    assert len(selected.candidates) == {"ambiguous": 2, "pending": 1}.get(case, 0)


def test_partial_domains_and_colliding_namespaces_stay_reference_pending() -> None:
    sources = SyntheticSources()
    current = sources.use()
    partial = definition("T0123456789", current).model_copy(update={"uses": None})
    for inputs, namespace in (
        ((legacy(partial),), "synthetic-old-v1"),
        (
            (
                legacy(definition(partial.template_id, current)),
                legacy(
                    definition(partial.template_id, current), namespace="other-old-v1"
                ),
            ),
            None,
        ),
    ):
        report = build(
            (ruling(applies_to=[partial.template_id]),),
            Rebuild(inputs, (current,), sources.verify),
        )
        result = report.resolutions[0]
        assert result.level == "reference"
        assert result.reason == "unknown_legacy_scope"
        assert result.legacy.namespace == namespace
        assert result.legacy.occurrence is None
        assert result.candidates == ()


@pytest.mark.parametrize(
    "case",
    [
        "empty",
        "duplicate",
        "missing_member",
        "extra_member",
        "unknown_definition",
        "duplicate_definition",
        "duplicate_namespace",
    ],
)
def test_invalid_old_domains_are_errors_instead_of_pending(case: str) -> None:
    sources = SyntheticSources()
    a, b, c = sources.use("a"), sources.use("b"), sources.use("c")
    full = definition("T0123456789", a, b)
    invalid = full
    expected = (full,)
    if case == "empty":
        invalid = full.model_copy(update={"uses": ()})
        expected = (invalid,)
    elif case == "duplicate":
        invalid = definition(full.template_id, a, a)
        expected = (invalid,)
    elif case == "missing_member":
        invalid = definition(full.template_id, a)
    elif case == "extra_member":
        invalid = definition(full.template_id, a, b, c)
    elif case == "unknown_definition":
        invalid = full.model_copy(update={"template_id": "T9999999999"})
    old = legacy(invalid, expected=expected)
    if case == "duplicate_definition":
        old = legacy(full, full)
    inputs = (old, old) if case == "duplicate_namespace" else (old,)
    with pytest.raises(ValueError, match=r"domain|definition|namespace"):
        Rebuild(inputs, (a, b, c), sources.verify)


def test_wrong_current_owner_and_variant_are_source_errors() -> None:
    sources = SyntheticSources()
    valid = sources.use()
    borrowed = replace(
        valid,
        binding=valid.binding.model_copy(
            update={
                "source": valid.binding.source.model_copy(
                    update={
                        "owner": FaceRevisionOwner(
                            kind="face_revision", revision_id="absent"
                        )
                    }
                )
            }
        ),
    )
    with pytest.raises(ValueError, match="borrowed"):
        Rebuild((), (borrowed,), sources.verify)
    with pytest.raises(ValueError, match="Duplicate current"):
        Rebuild((), (valid, valid), sources.verify)
    malformed = replace(
        valid,
        binding=valid.binding.model_copy(update={"frame_id": "frame:" + "a" * 64}),
    )
    with pytest.raises(ValueError, match="verified source use"):
        Rebuild((), (malformed,), sources.verify)


def test_verified_new_frame_ids_can_be_old_ids_without_a_prefix_heuristic() -> None:
    sources = SyntheticSources()
    current = sources.use()
    old = definition(current.frame.id, current)
    report = build(
        (ruling(applies_to=[old.template_id]),),
        Rebuild((legacy(old),), (current,), sources.verify),
    )
    assert report.resolutions[0].status == "resolved"
    assert report.payload()["legacy_namespaces"] == [
        {
            "code": "synthetic-old-v1",
            "id_recipe": "synthetic-id-list-v1",
            "normalizer_version": "synthetic-v0",
        }
    ]


def test_parameter_differences_keep_every_family_without_copying_string_operands() -> (
    None
):
    sources = SyntheticSources()
    a, b = sources.use("a"), sources.use("b")
    first = definition("T0123456789", a)
    second = definition("T1111111111", b)
    assert first.uses is not None
    assert second.uses is not None
    first = first.model_copy(
        update={
            "uses": (
                first.uses[0].model_copy(
                    update={"parameters": {"old_name": "Synthetic protected wording"}}
                ),
            )
        }
    )
    second = second.model_copy(update={"semantic_variant": "synthetic.other"})
    report = build(
        (ruling(applies_to=[first.template_id, second.template_id]),),
        Rebuild((legacy(first, second),), (a, b), sources.verify),
    )
    assert len(report.families) == 1
    assert report.families[0]["legacy_variants"] == ["synthetic.old", "synthetic.other"]
    assert b"Synthetic protected wording" not in canonical(report.payload())
    members = report.families[0]["families"]
    assert isinstance(members, list)
    assert any(isinstance(m, dict) and m["parameter_changes"] for m in members)


def test_candidate_frame_fk_is_checked_even_when_resolution_is_pending(
    stored: tuple[Database, Frame, SourceBinding],
) -> None:
    db, _, _ = stored
    sources = SyntheticSources()
    current = sources.use(variant=None)
    rebuild = Rebuild(
        (legacy(definition("T0123456789", current)),), (current,), sources.verify
    )
    with (
        db.transaction(),
        pytest.raises(ValueError, match="candidate frame is missing"),
    ):
        write(db, (ruling(applies_to=["T0123456789"]),), "a" * 40, rebuild=rebuild)


def test_stored_pending_candidates_retain_exact_source_and_never_become_active(
    stored: tuple[Database, Frame, SourceBinding],
) -> None:
    db, frame, binding = stored
    current = FrameUse(
        frame, binding, Scope(role=frame.role, domain="synthetic_effect")
    )

    def verify(use: FrameUse) -> None:
        if use != current:
            raise ValueError("Unknown current source")
        use.binding.verify(use.frame, {})

    rebuild = Rebuild((legacy(definition("T0123456789", current)),), (current,), verify)
    with db.transaction():
        report = write(
            db, (ruling(applies_to=["T0123456789"]),), "a" * 40, rebuild=rebuild
        )
    assert report.resolutions[0].reason == "unsupported_relation"
    assert report.resolutions[0].candidates == (current.target(),)
    assert db.rows("ruling_resolution")[0].values["frame_id"] is None


def test_candidate_source_use_must_exist_in_the_same_build(
    request: pytest.FixtureRequest,
) -> None:
    parent: tuple[Database, Frame, SourceBinding] = request.getfixturevalue("unbound")
    db, frame, binding = parent
    current = FrameUse(
        frame, binding, Scope(role=frame.role, domain="synthetic_effect")
    )

    def verify(use: FrameUse) -> None:
        if use != current:
            raise ValueError("Unknown current source")
        use.binding.verify(use.frame, {})

    rebuild = Rebuild((legacy(definition("T0123456789", current)),), (current,), verify)
    with db.transaction(), pytest.raises(ValueError, match="source use is missing"):
        write(db, (ruling(applies_to=["T0123456789"]),), "b" * 40, rebuild=rebuild)
