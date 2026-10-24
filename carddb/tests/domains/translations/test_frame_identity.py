"""Frame addresses follow closed source meaning instead of release bookkeeping."""

from copy import deepcopy
from dataclasses import replace

import pytest

from sve_carddb.contracts.four_layer import hash_payload
from sve_carddb.contracts.source_binding import CodeDomain, QuantityDomain
from sve_carddb.domains.translations.four_layer_context import Registry
from sve_carddb.domains.translations.four_layer_derivation import (
    DerivedField,
    derive_source,
)
from sve_carddb.domains.translations.four_layer_identities import context_variant
from sve_carddb.domains.translations.four_layer_identity import (
    OMISSIONS,
    RESOLVED,
    seal,
)
from sve_carddb.domains.translations.four_layer_semantics import CardContext
from sve_carddb.domains.translations.four_layer_units import unit_meaning

from .test_four_layer_classification import classifier, source
from .test_four_layer_event_sources import prepared_case
from .test_four_layer_n1 import n1_classifier
from .test_four_layer_n2_parameters import engine as parameter_classifier


def prepared() -> DerivedField:
    raw = "自分の手札の仮族・フォロワー1枚を選ぶ。"
    return derive_source(raw, source(raw), n1_classifier())


def test_release_label_does_not_change_frame_or_dependent_context() -> None:
    derived = prepared()
    frame, binding = derived.bind(0)
    changed = frame.model_copy(
        update={
            "source": frame.source.model_copy(
                update={"normalizer_version": "four-layer-jp-next"}
            )
        }
    )
    canonical = derived.field.parts[0].canonical_source
    changed.verify(canonical)
    assert hash_payload(changed.payload(canonical)) == frame.content_hash
    assert context_variant((binding,)) == context_variant(
        (binding.model_copy(update={"frame_id": changed.id}),)
    )


def test_referenced_domain_meaning_changes_identity_without_renaming_it() -> None:
    derived = prepared()
    before, _ = derived.bind(0)
    after, _ = replace(
        derived,
        source_domains={
            **derived.source_domains,
            "player.relative.v1": CodeDomain(type="Player", codes=("self",)),
        },
    ).bind(0)
    assert before.id != after.id
    assert before.leaf_schema == after.leaf_schema


def test_unreferenced_domain_does_not_change_frame() -> None:
    derived = prepared()
    before, _ = derived.bind(0)
    after, _ = replace(
        derived,
        source_domains={
            **derived.source_domains,
            "player.unused.v1": CodeDomain(type="Player", codes=("self",)),
        },
    ).bind(0)
    assert before.id == after.id


def test_missing_referenced_definition_fails_closed() -> None:
    derived = prepared()
    with pytest.raises(ValueError, match="Unclosed frame semantic domain"):
        replace(derived, source_domains={}).bind(0)


def test_same_address_with_different_semantic_payload_is_rejected() -> None:
    derived = prepared()
    frame, _ = derived.bind(0)
    other = frame.model_copy(
        update={"source": frame.source.model_copy(update={"semantic_definitions": {}})}
    )
    canonical = derived.field.parts[0].canonical_source
    with pytest.raises(ValueError, match="hash collision"):
        frame.verify_same_identity(canonical, other, canonical)
    with pytest.raises(ValueError, match="not sealed"):
        other.verify(canonical)


def test_expression_definition_changes_address_but_unrelated_registry_addition_does_not() -> (
    None
):
    raw = "相手にXダメージ。Xは「これの攻撃力」である。"
    derived = derive_source(raw, source(raw), n1_classifier())
    frame, binding = derived.bind(0)
    canonical = derived.field.parts[0].canonical_source
    registry = derived.registry
    descriptor = registry.descriptors["read.object.attack.v1"]
    changed = Registry(
        tuple(
            replace(d, measure="health") if d.id == descriptor.id else d
            for d in registry.descriptors.values()
        )
    )
    after, _ = seal(frame, binding, canonical, derived.source_domains, changed)
    assert after.id != frame.id
    unrelated = registry.extend((replace(descriptor, id="read.unrelated.v1"),))
    same, _ = seal(frame, binding, canonical, derived.source_domains, unrelated)
    assert same.id == frame.id


def test_unknown_expression_definition_cannot_keep_an_old_identity() -> None:
    raw = "相手にXダメージ。Xは「これの攻撃力」である。"
    derived = derive_source(raw, source(raw), n1_classifier())
    frame, binding = derived.bind(0)
    with pytest.raises(ValueError, match="Unclosed frame semantic expression"):
        seal(
            frame,
            binding,
            derived.field.parts[0].canonical_source,
            derived.source_domains,
            Registry(()),
        )


def test_omission_contract_change_cannot_silently_keep_identity(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    raw = "自分の墓場のフォロワー７枚を選び、手札に加える。"
    derived = derive_source(raw, source(raw), n1_classifier())
    before, _ = derived.bind(0)
    rule = "owner.self_hand_destination.v1"
    omissions = before.source.semantic_definitions["omissions"]
    assert isinstance(omissions, dict)
    assert rule in omissions
    monkeypatch.setitem(
        OMISSIONS, rule, {"owner": "opponent", "reason": "actor_hand_destination"}
    )
    with pytest.raises(ValueError, match="contradicts omission definition"):
        derived.bind(0)
    monkeypatch.delitem(OMISSIONS, rule)
    with pytest.raises(ValueError, match="Unclosed frame omission"):
        derived.bind(0)


def test_resolved_definition_is_part_of_identity(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    raw = "1枚引く。"
    facts = CardContext(
        source(raw), "c:synthetic", "f:synthetic", "normal", "spell", True, 0
    )
    derived = derive_source(raw, facts.source, classifier(), context=facts)
    frame, binding = derived.bind(0)
    assert frame.semantic_variant.key == "draw_exact_spell.v1"
    canonical = derived.field.parts[0].canonical_source
    before, _ = seal(
        frame, binding, canonical, derived.source_domains, derived.registry
    )
    monkeypatch.setitem(
        RESOLVED, "draw_exact_spell.v1", {"kind": "draw", "player": "opponent"}
    )
    after, _ = seal(frame, binding, canonical, derived.source_domains, derived.registry)
    assert before.id != after.id
    monkeypatch.delitem(RESOLVED, "draw_exact_spell.v1")
    with pytest.raises(ValueError, match="Unclosed resolved frame semantics"):
        seal(frame, binding, canonical, derived.source_domains, derived.registry)


@pytest.mark.parametrize(
    ("unit", "role", "expected"),
    [
        ("枚", "count", "object_count"),
        ("体", "count", "object_count"),
        ("つ", "count", "object_count"),
        ("つ", "counter_amount", "counter_count"),
        ("個", "count", "counter_count"),
        ("人", "count", "player_count"),
        ("PP", "cost", "play_points"),
        ("ＰＰ", "cost", "play_points"),
    ],
)
def test_closed_unit_meanings(unit: str, role: str, expected: str) -> None:
    assert unit_meaning(unit, role) == expected


def test_unknown_unit_cannot_silently_inherit_identity() -> None:
    with pytest.raises(ValueError, match="Unclosed source unit"):
        unit_meaning("unknown", "count")


def test_equal_resolved_contract_merges_values_and_pending_requires_new_identity() -> (
    None
):
    frames = []
    for raw in ("1枚引く。", "3枚引く。"):
        facts = CardContext(
            source(raw), "c:synthetic", "f:synthetic", "normal", "spell", True, 0
        )
        derived = derive_source(raw, facts.source, classifier(), context=facts)
        frame, _ = derived.bind(0)
        frames.append(frame)
        pending, _ = derive_source(raw, facts.source, classifier()).bind(0)
        assert pending.semantic_variant.state == "pending"
        assert pending.id != frame.id
    assert frames[0].id == frames[1].id


def test_source_capture_change_cannot_keep_frame_identity() -> None:
    raw = "相手にXダメージ。Xは「これの攻撃力」である。"
    derived = derive_source(raw, source(raw), n1_classifier())
    frame, binding = derived.bind(0)
    imports = frame.projection.imports
    port = next(p for p in imports if p.evidence is not None)
    assert port.evidence is not None
    changed = port.model_copy(
        update={"evidence": port.evidence.model_copy(update={"capture": "selection"})}
    )
    assert changed != port
    projection = frame.projection.model_copy(
        update={
            "imports": tuple(changed if p.name == port.name else p for p in imports)
        }
    )
    after, _ = seal(
        frame.model_copy(update={"projection": projection}),
        binding,
        derived.field.parts[0].canonical_source,
        derived.source_domains,
        derived.registry,
    )
    assert after.id != frame.id


def test_collision_check_compares_canonical_bytes_not_python_numeric_equality() -> None:
    derived = prepared()
    frame, _ = derived.bind(0)
    before = frame.model_copy(
        update={
            "source": frame.source.model_copy(
                update={"semantic_definitions": {"flag": True}}
            )
        }
    )
    after = frame.model_copy(
        update={
            "source": frame.source.model_copy(
                update={"semantic_definitions": {"flag": 1}}
            )
        }
    )
    canonical = derived.field.parts[0].canonical_source
    assert before.source.semantic_definitions == after.source.semantic_definitions
    with pytest.raises(ValueError, match="hash collision"):
        before.verify_same_identity(canonical, after, canonical)


def test_source_replay_rejects_bool_replaced_by_equal_python_integer() -> None:
    raw = "自分の手札の仮族・フォロワー1枚を選ぶ。"
    derived = prepared()
    frame, binding = derived.bind(0)
    definitions = deepcopy(frame.source.semantic_definitions)
    domains = definitions["domains"]
    assert isinstance(domains, dict)
    domain = domains["quantity.selection_count.constant.v1"]
    assert isinstance(domain, dict)
    assert isinstance(domain["constant_only"], bool)
    domain["constant_only"] = int(domain["constant_only"])
    forged = frame.model_copy(
        update={
            "source": frame.source.model_copy(
                update={"semantic_definitions": definitions}
            )
        }
    )
    checksum = hash_payload(forged.payload(derived.field.parts[0].canonical_source))
    forged = forged.model_copy(
        update={"id": "frame:" + checksum, "content_hash": checksum}
    )
    changed_binding = binding.model_copy(update={"frame_id": forged.id})
    changed_binding = changed_binding.model_copy(
        update={"id": "bind:" + hash_payload(changed_binding.payload())}
    )
    assert forged.source.semantic_definitions == frame.source.semantic_definitions
    with pytest.raises(ValueError, match="complete current source construction"):
        derived.verify_binding(raw, n1_classifier(), forged, changed_binding)


def test_optional_expression_is_closed_without_a_bound_value() -> None:
    raw = "相手にXダメージ。Xは「これの攻撃力」である。"
    derived = derive_source(raw, source(raw), n1_classifier())
    frame, binding = derived.bind(0)
    slot = next(s for s in frame.leaf_schema.slots if s.expression is not None)
    assert slot.expression is not None
    frame = frame.model_copy(
        update={
            "leaf_schema": frame.leaf_schema.model_copy(
                update={
                    "slots": tuple(
                        s.model_copy(update={"required": False})
                        if s.name == slot.name
                        else s
                        for s in frame.leaf_schema.slots
                    )
                }
            )
        }
    )
    binding = binding.model_copy(
        update={
            "values": {k: v for k, v in binding.values.items() if k != slot.name},
            "occurrences": tuple(o for o in binding.occurrences if o.slot != slot.name),
        }
    )
    canonical = derived.field.parts[0].canonical_source
    before, _ = seal(
        frame, binding, canonical, derived.source_domains, derived.registry
    )
    identifier = slot.expression.expression_id
    changed = Registry(
        tuple(
            replace(d, measure="health") if d.id == identifier else d
            for d in derived.registry.descriptors.values()
        )
    )
    after, _ = seal(frame, binding, canonical, derived.source_domains, changed)
    assert before.id != after.id
    with pytest.raises(ValueError, match="Unclosed frame semantic expression"):
        seal(frame, binding, canonical, derived.source_domains, Registry(()))


def test_producer_cannot_change_an_omitted_owner_without_changing_contract() -> None:
    raw = "自分の墓場のフォロワー７枚を選び、手札に加える。"
    derived = derive_source(raw, source(raw), n1_classifier())
    frame, binding = derived.bind(0)
    occurrence = next(o for o in binding.occurrences if o.resolution_rule)
    changed = binding.model_copy(
        update={"values": {**binding.values, occurrence.slot: "opponent"}}
    )
    with pytest.raises(ValueError, match="contradicts omission definition"):
        seal(
            frame,
            changed,
            derived.field.parts[0].canonical_source,
            derived.source_domains,
            derived.registry,
        )


def test_producer_cannot_change_an_omitted_amount_without_changing_contract() -> None:
    raw = "【土の秘術】これは仮の効果を行う。"
    derived = derive_source(raw, source(raw), parameter_classifier())
    frame, binding = derived.bind(0)
    occurrence = next(
        o
        for o in binding.occurrences
        if o.resolution_rule == "earth_rite.omitted_amount.v1"
    )
    changed = binding.model_copy(
        update={"values": {**binding.values, occurrence.slot: 2}}
    )
    with pytest.raises(ValueError, match="contradicts omission definition"):
        seal(
            frame,
            changed,
            derived.field.parts[0].canonical_source,
            derived.source_domains,
            derived.registry,
        )


def test_empty_closure_cannot_verify_even_with_a_matching_digest() -> None:
    derived = prepared()
    frame, _ = derived.bind(0)
    frame = frame.model_copy(
        update={"source": frame.source.model_copy(update={"semantic_definitions": {}})}
    )
    canonical = derived.field.parts[0].canonical_source
    checksum = hash_payload(frame.payload(canonical))
    frame = frame.model_copy(
        update={"id": "frame:" + checksum, "content_hash": checksum}
    )
    with pytest.raises(ValueError, match="not sealed"):
        frame.verify(canonical)


def test_quantity_mode_set_reordering_preserves_identity() -> None:
    derived = prepared()
    frame, _ = derived.bind(0)
    domains = {
        name: value.model_copy(
            update={
                "modes": tuple(reversed(value.modes)),
                "imports": tuple(reversed(value.imports)),
            }
        )
        if isinstance(value, QuantityDomain)
        else value
        for name, value in derived.source_domains.items()
    }
    changed, _ = replace(derived, source_domains=domains).bind(0)
    assert changed.id == frame.id


def test_named_descriptor_import_reordering_preserves_identity() -> None:
    derived, frame, binding = prepared_case(
        "これが手札から場に出ていたなら、７枚引く。"
    )
    changed = Registry(
        tuple(
            replace(d, imports=tuple(reversed(d.imports)))
            for d in derived.registry.descriptors.values()
        )
    )
    after, _ = seal(
        frame,
        binding,
        derived.field.parts[0].canonical_source,
        derived.source_domains,
        changed,
    )
    assert after.id == frame.id
