"""Fixed four-layer identities and closed target shapes through shared public models."""

from copy import deepcopy
from pathlib import Path
from unicodedata import normalize

import pytest
from pydantic import JsonValue, TypeAdapter, ValidationError

from sve_carddb.contracts.annotations import AnnotationSet
from sve_carddb.contracts.four_layer import (
    Bound,
    CardNameReference,
    Constant,
    Frame,
    GlossaryReference,
    LeafSlot,
    QuantitySpec,
    Target,
    VocabularyReference,
    hash_payload,
)
from sve_carddb.contracts.source_binding import (
    LayoutDomain,
    QuantityDomain,
    ReferenceDomain,
    SourceBinding,
    SourceDescriptor,
    SourceSpan,
    TracePiece,
    ZoneDomain,
    verify_partition,
    verify_trace,
    verify_value,
)
from sve_carddb.core.json import array, canonical, digest, object_value, string

CASES = object_value(
    TypeAdapter(JsonValue).validate_json(
        (
            Path(__file__).resolve().parents[3]
            / "docs/schema/domains/four-layer-cases.json"
        ).read_bytes()
    )
)
IDENTITY = tuple(
    object_value(c)
    for c in array(CASES["cases"])
    if object_value(c)["operation"] == "frame_identity"
)


def changed(
    base: dict[str, JsonValue], changes: dict[str, JsonValue]
) -> dict[str, JsonValue]:
    """Preserve the fixture's original values when testing an independent mutation."""
    result = deepcopy(base)
    for pointer, value in changes.items():
        parts = pointer.split("/")[1:]
        current: JsonValue = result
        for part in parts[:-1]:
            current = (
                array(current)[int(part)]
                if isinstance(current, list)
                else object_value(current)[part]
            )
        if isinstance(current, list):
            current[int(parts[-1])] = value
        else:
            object_value(current)[parts[-1]] = value
    return result


def frame(value: dict[str, JsonValue]) -> Frame:
    """The fixed oracle is external to these models and checked in the assertions."""
    semantic = object_value(value["semantic"])
    checksum = hash_payload(semantic)
    data: dict[str, JsonValue] = {
        "id": "frame:" + checksum,
        "source": {
            "source_lang": semantic["source_lang"],
            "normalizer_version": semantic["normalizer_version"],
            "canonical_hash": digest(string(semantic["canonical_source"]).encode())[7:],
        },
        "role": semantic["role"],
        "leaf_schema": semantic["leaf_schema"],
        "semantic_variant": semantic["semantic_variant"],
        "projection": object_value(semantic["projection"])
        | object_value(value["projection_interface"]),
        "content_hash": checksum,
    }
    result = Frame.model_validate_json(canonical(data))
    result.verify(string(semantic["canonical_source"]))
    return result


@pytest.mark.parametrize(
    "case", IDENTITY, ids=lambda c: string(c["group"]) + "/" + string(c["id"])
)
def test_fixed_frame_and_interface_hashes(case: dict[str, JsonValue]) -> None:
    inputs = object_value(case["input"])
    original = object_value(object_value(CASES["fixtures"])[string(inputs["fixture"])])
    updated = changed(original, object_value(inputs["changes"]))
    before, after = frame(original), frame(updated)
    expected = object_value(case["expected"])
    assert before.id == expected["before_id"]
    assert after.id == expected["after_id"]
    assert (before.id == after.id) == expected["frame_id_equal"]
    before_interface = before.projection.interface_key(before.id)
    after_interface = after.projection.interface_key(after.id)
    assert before_interface == expected["before_interface_hash"]
    assert after_interface == expected["after_interface_hash"]
    assert (before_interface == after_interface) == expected["interface_key_equal"]


def test_card_np_is_closed_and_cannot_swallow_optional_random_or_scope() -> None:
    base: dict[str, JsonValue] = {
        "format": 1,
        "nodes": [
            {
                "kind": "NP",
                "constructor": "CardNP",
                "args": {
                    "kind": {"slot": "kind"},
                    "quantity": {"slot": "quantity"},
                },
            }
        ],
    }
    target = Target.model_validate_json(canonical(base))
    fixture = object_value(object_value(CASES["fixtures"])["identity_base"])
    schema = frame(fixture).leaf_schema
    with pytest.raises(ValueError, match="Target must use every required leaf"):
        target.verify(schema, "zh-Hant", {})
    for key in ("optional", "random", "scope"):
        invalid = changed(base, {"/nodes/0/args/" + key: "unknown"})
        with pytest.raises(ValidationError):
            Target.model_validate_json(canonical(invalid))


@pytest.mark.parametrize("optional", ["owner", "zone", "traits", "class", "token"])
def test_card_np_optional_arguments_reject_explicit_null(optional: str) -> None:
    with pytest.raises(ValidationError):
        Target.model_validate_json(
            canonical(
                {
                    "format": 1,
                    "nodes": [
                        {
                            "kind": "NP",
                            "constructor": "CardNP",
                            "args": {
                                "kind": {"slot": "kind"},
                                "quantity": {"slot": "quantity"},
                                optional: None,
                            },
                        }
                    ],
                }
            )
        )


def test_literal_nodes_do_not_parse_old_placeholders() -> None:
    target = Target.model_validate_json(
        canonical(
            {
                "format": 1,
                "nodes": [
                    {"kind": "Literal", "text": "{{quantity}}"},
                ],
            }
        )
    )
    fixture = object_value(object_value(CASES["fixtures"])["identity_base"])
    with pytest.raises(ValueError, match="Target must use every required leaf"):
        target.verify(frame(fixture).leaf_schema, "zh-Hant", {})


def test_np_and_import_roundtrip_keep_aliases_and_optional_omissions() -> None:
    card: dict[str, JsonValue] = {
        "kind": "NP",
        "constructor": "CardNP",
        "args": {
            "kind": {"slot": "kind"},
            "quantity": {"slot": "quantity"},
            "class": {"slot": "class"},
        },
    }
    target = Target.model_validate_json(
        canonical(
            {
                "format": 1,
                "nodes": [
                    card,
                    {
                        "kind": "NP",
                        "constructor": "UnionNP",
                        "args": {"branches": [card, card]},
                    },
                ],
            }
        )
    )
    serialized = target.model_dump_json()
    assert Target.model_validate_json(serialized) == target
    nodes = array(
        object_value(TypeAdapter(JsonValue).validate_json(serialized))["nodes"]
    )
    args = object_value(object_value(nodes[0])["args"])
    assert args["class"] == {"slot": "class"}
    assert not {"owner", "zone", "token", "class_"} & args.keys()
    assert "quantity" not in object_value(object_value(nodes[1])["args"])
    bound = Bound.model_validate_json(b'{"kind":"bound","import":"count"}')
    assert Bound.model_validate_json(bound.model_dump_json()) == bound
    assert bound.model_dump(mode="json") == {"kind": "bound", "import": "count"}


def test_named_source_domains_accept_each_alternative_and_reject_unknown_names() -> (
    None
):
    slot = LeafSlot.model_validate_json(
        b'{"name":"zone","type":"ZoneSet","role":"source_zone",'
        b'"domain":{"values":["battlefield","hand"],"min":null,"max":null},'
        b'"required":true,"occurrences":[{"start":0,"end":1}]}'
    )
    domains = {
        "battlefield": ZoneDomain(type="ZoneSet", zones=("battlefield",)),
        "hand": ZoneDomain(type="ZoneSet", zones=("hand",)),
    }
    verify_value(slot, ("hand",), domains)
    verify_value(slot, ("battlefield",), domains)
    with pytest.raises(ValueError, match="outside every registered"):
        verify_value(slot, ("deck",), domains)
    with pytest.raises(ValueError, match="Unknown or incorrectly typed"):
        verify_value(slot, ("hand",), {"hand": domains["hand"]})


def test_layout_values_do_not_change_the_registered_domain_or_frame_schema() -> None:
    slot = LeafSlot.model_validate_json(
        b'{"name":"layout","type":"LiteralLayout","role":"layout",'
        b'"domain":{"values":["source_whitespace"],"min":null,"max":null},'
        b'"required":true,"occurrences":[{"start":0,"end":1}]}'
    )
    domains = {"source_whitespace": LayoutDomain(type="LiteralLayout")}
    for value in (" ", "\n", "\r\n", "\t  \r\n"):
        verify_value(slot, value, domains)
    with pytest.raises(ValueError, match="outside every registered"):
        verify_value(slot, " rules text ", domains)
    with pytest.raises(ValueError, match="outside every registered"):
        verify_value(slot, "", domains)


@pytest.mark.parametrize(
    "case",
    [
        object_value(c)
        for c in array(CASES["cases"])
        if object_value(c)["operation"] == "source_positions"
    ],
    ids=lambda c: string(c["id"]),
)
def test_fixed_source_positions(case: dict[str, JsonValue]) -> None:
    inputs = object_value(case["input"])
    fixture = object_value(object_value(CASES["fixtures"])[string(inputs["fixture"])])
    value = changed(fixture, object_value(inputs.get("changes", {})))

    def check() -> None:
        spans = tuple(
            SourceSpan.model_validate_json(
                canonical(
                    {
                        "role": object_value(s)["role"],
                        "segments": [
                            {
                                "start": object_value(s)["start"],
                                "end": object_value(s)["end"],
                            }
                        ],
                        "anchor": None,
                    }
                )
            )
            for s in array(value["segments"])
        )
        raw, normalized = string(value["raw"]), string(value["canonical"])
        verify_partition(raw, spans)
        trace = tuple(
            TracePiece.model_validate_json(canonical(p)) for p in array(value["trace"])
        )
        complete = SourceSpan.model_validate_json(
            canonical(
                {
                    "role": "body",
                    "segments": [{"start": 0, "end": len(raw)}],
                    "anchor": None,
                }
            )
        )
        verify_trace(
            raw,
            normalized,
            complete,
            trace,
            {
                "identity": lambda s: s,
                "nfkc_digit": lambda s: normalize("NFKC", s),
                "layout": lambda _s: "",
            },
        )

    if object_value(case["expected"])["result"] == "reject":
        with pytest.raises(
            ValueError, match=r"Source partition|Layout|Trace|spans must"
        ):
            check()
    else:
        check()
        assert (
            len(string(value["raw"]))
            == object_value(case["expected"])["raw_codepoints"]
        )
        assert (
            len(string(value["canonical"]))
            == object_value(case["expected"])["canonical_codepoints"]
        )


@pytest.mark.parametrize(
    "case",
    [
        object_value(c)
        for c in array(CASES["cases"])
        if object_value(c)["operation"] == "leaf_domain"
        and "type" in object_value(object_value(c)["input"])
    ],
    ids=lambda c: string(c["id"]),
)
def test_fixed_numeric_domains(case: dict[str, JsonValue]) -> None:
    inputs = object_value(case["input"])
    slot = LeafSlot.model_validate_json(
        canonical(
            {
                "name": "n",
                "type": inputs["type"],
                "role": inputs["role"],
                "domain": {"values": [], "min": inputs["min"], "max": inputs["max"]},
                "required": True,
                "occurrences": [{"start": 0, "end": 1}],
            }
        )
    )
    value = inputs["value"]
    assert isinstance(value, int)
    if object_value(case["expected"])["result"] == "reject":
        with pytest.raises(ValueError, match="Numeric leaf"):
            verify_value(slot, value)
    else:
        verify_value(slot, value)


def test_reference_catalog_growth_preserves_frame_and_rejects_unadopted_values() -> (
    None
):
    slot = LeafSlot.model_validate_json(
        canonical(
            {
                "name": "name",
                "type": "CardName",
                "role": "declared_name",
                "domain": {"values": ["card_name.any.v1"], "min": None, "max": None},
                "required": True,
                "occurrences": [{"start": 0, "end": 1}],
            }
        )
    )
    first = CardNameReference(kind="card_name", term_id="term:first")
    other = CardNameReference(kind="card_name", term_id="term:other")
    before = canonical(slot.model_dump(mode="json"))
    for references in ((first,), (first, other)):
        domain = ReferenceDomain(
            type="CardName", category="card_name", references=references
        )
        verify_value(slot, first, {"card_name.any.v1": domain})
        assert canonical(slot.model_dump(mode="json")) == before
    for invalid in (other, GlossaryReference(kind="glossary", key="term:first")):
        with pytest.raises(ValueError, match="outside every registered"):
            verify_value(
                slot,
                invalid,
                {
                    "card_name.any.v1": ReferenceDomain(
                        type="CardName", category="card_name", references=(first,)
                    ),
                },
            )
    with pytest.raises(ValueError, match="Unknown or incorrectly typed"):
        verify_value(slot, first)
    with pytest.raises(ValueError, match="wrong reference kind"):
        ReferenceDomain(
            type="CardKind",
            category="type",
            references=(
                VocabularyReference(kind="vocabulary", key=("class", "neutral")),
            ),
        )


def test_constant_quantity_domain_rejects_all_and_bound_even_with_registered_import() -> (
    None
):
    slot = LeafSlot.model_validate_json(
        canonical(
            {
                "name": "n",
                "type": "QuantitySpec",
                "role": "selection_count",
                "domain": {
                    "values": ["quantity.selection_count.constant.v1"],
                    "min": None,
                    "max": None,
                },
                "required": True,
                "occurrences": [{"start": 0, "end": 1}],
            }
        )
    )
    domain = QuantityDomain(
        type="QuantitySpec",
        modes=("exact", "up_to", "all"),
        imports=("received",),
        expressions=(),
        constant_only=True,
    )
    domains = {"quantity.selection_count.constant.v1": domain}
    verify_value(
        slot,
        QuantitySpec(mode="up_to", expr=Constant(kind="constant", value=2)),
        domains,
    )
    for invalid in (
        QuantitySpec(mode="all", expr=None),
        QuantitySpec(mode="exact", expr=Bound(kind="bound", **{"import": "received"})),
    ):
        with pytest.raises(ValueError, match="outside every registered"):
            verify_value(slot, invalid, domains)


def annotation_set(text: str, occurrences: JsonValue) -> AnnotationSet:
    """Build the logical recipe used by the external public golden vectors."""
    unit = "t:zh-Hant:" + digest(text.encode())[7:23]
    payload: dict[str, JsonValue] = {
        "recipe": "annotation-v1",
        "text_unit_id": unit,
        "occurrences": occurrences,
    }
    result = AnnotationSet.model_validate_json(
        canonical(
            {
                "id": "ann:" + hash_payload(payload),
                "text_unit_id": unit,
                "occurrences": occurrences,
            }
        )
    )
    result.verify(unit, "zh-Hant", text)
    return result


def test_card_name_protects_inner_terms_and_concepts_do_not_deduplicate_by_text() -> (
    None
):
    fixture = object_value(object_value(CASES["fixtures"])["annotation_base"])
    original = annotation_set(string(fixture["text"]), fixture["occurrences"])
    assert len(original.occurrences) == 2
    assert [(r.start, r.end) for r in original.occurrences[1].ranges] == [(5, 7)]
    replaced = changed(
        fixture,
        {"/occurrences/1/reference": {"kind": "glossary", "key": "term:fixture.other"}},
    )
    other = annotation_set(string(replaced["text"]), replaced["occurrences"])
    assert original.text_unit_id == other.text_unit_id
    assert original.id != other.id


def test_annotation_rejects_nested_terms_and_reordered_occurrences() -> None:
    fixture = object_value(object_value(CASES["fixtures"])["annotation_base"])
    overlapping = changed(fixture, {"/occurrences/1/ranges": [{"start": 2, "end": 4}]})
    with pytest.raises(ValueError, match=r"Annotations must|spans must"):
        annotation_set(string(overlapping["text"]), overlapping["occurrences"])
    reversed_ = list(reversed(array(fixture["occurrences"])))
    with pytest.raises(ValueError, match="Annotation ordinals"):
        annotation_set(string(fixture["text"]), reversed_)


def binding_sample() -> tuple[Frame, SourceBinding, str, str]:
    """Use one number next to a non-BMP scalar and layout to exercise source identity."""
    raw, normalized = "計数２😀\r\n", "計数N😀"
    semantic: dict[str, JsonValue] = {
        "recipe": "frame-v1",
        "source_lang": "ja",
        "canonical_source": normalized,
        "normalizer_version": "fixture-normalizer-v1",
        "role": "body",
        "leaf_schema": {
            "format": 2,
            "slots": [
                {
                    "name": "n",
                    "type": "Nat",
                    "role": "count",
                    "domain": {"values": [], "min": 0, "max": 9007199254740991},
                    "required": True,
                    "occurrences": [{"start": 2, "end": 3}],
                }
            ],
        },
        "semantic_variant": {
            "state": "resolved",
            "key": "fixture_count",
            "scope": None,
        },
        "projection": {
            "projection_kind": "ability_body",
            "discriminator": "fixture_count",
        },
    }
    definition = frame(
        {
            "semantic": semantic,
            "projection_interface": {"scopes": [], "imports": [], "exports": []},
        }
    )
    source: dict[str, JsonValue] = {
        "owner": {"kind": "face_revision", "revision_id": "revision:fixture"},
        "field": "effect",
        "ordinal": None,
        "source_unit_id": "t:ja:" + digest(raw.encode())[7:23],
        "source_hash": digest(raw.encode())[7:],
        "source_ref": {
            "batch_id": "batch:fixture",
            "source_version_id": "version:fixture",
            "parser": "fixture-parser-v1",
            "locator": "/faces/0/effect",
            "text_hash": digest(raw.encode())[7:],
        },
    }
    source_span: dict[str, JsonValue] = {
        "role": "body",
        "segments": [{"start": 0, "end": 4}],
        "anchor": None,
    }
    occurrence: dict[str, JsonValue] = {
        "owner": source["owner"],
        "field": "effect",
        "ordinal": None,
        "source_hash": source["source_hash"],
        "line_ordinal": 0,
        "role": "body",
        "segments": source_span["segments"],
    }
    values: dict[str, JsonValue] = {"n": 2}
    leaves: list[JsonValue] = [
        {
            "slot": "n",
            "ordinal": 0,
            "raw_spans": [{"start": 2, "end": 3}],
            "canonical_spans": [{"start": 2, "end": 3}],
            "source_unit": None,
            "source_presence": "explicit",
            "resolution_rule": None,
        }
    ]
    trace: list[JsonValue] = [
        {
            "raw_span": {"start": 0, "end": 2},
            "canonical_spans": [{"start": 0, "end": 2}],
            "rule": "identity",
        },
        {
            "raw_span": {"start": 2, "end": 3},
            "canonical_spans": [{"start": 2, "end": 3}],
            "rule": "decimal",
        },
        {
            "raw_span": {"start": 3, "end": 4},
            "canonical_spans": [{"start": 3, "end": 4}],
            "rule": "identity",
        },
    ]
    payload: dict[str, JsonValue] = {
        "recipe": "binding-v2",
        "occurrence": occurrence,
        "frame_id": definition.id,
        "source_span": source_span,
        "values": values,
        "occurrences": leaves,
        "trace": trace,
    }
    result = SourceBinding.model_validate_json(
        canonical(
            {
                "id": "bind:" + hash_payload(payload),
                "source": source,
                "ordinal": 0,
                "line_ordinal": 0,
                **{
                    k: payload[k]
                    for k in (
                        "frame_id",
                        "source_span",
                        "values",
                        "occurrences",
                        "trace",
                    )
                },
            }
        )
    )
    return definition, result, raw, normalized


def test_binding_archive_labels_preserve_identity_but_owner_is_not_interchangeable() -> (
    None
):
    definition, binding, raw, normalized = binding_sample()
    binding.verify(definition)
    binding.source.verify(binding.source, raw)
    verify_trace(
        raw,
        normalized,
        binding.source_span,
        binding.trace,
        {"identity": lambda s: s, "decimal": lambda _s: "N"},
    )
    data = binding.model_dump(mode="json")
    archive = changed(
        data,
        {
            "/source/source_ref/batch_id": "batch:renamed",
            "/source/source_ref/source_version_id": "version:renamed",
        },
    )
    SourceBinding.model_validate_json(canonical(archive)).verify(definition)
    another = changed(data, {"/source/owner/revision_id": "revision:other"})
    borrowed = SourceBinding.model_validate_json(canonical(another))
    with pytest.raises(ValueError, match="Binding identity"):
        borrowed.verify(definition)
    with pytest.raises(ValueError, match="exact owner field"):
        borrowed.source.verify(binding.source, raw)
    with pytest.raises(ValueError, match="stale exact bytes"):
        binding.source.verify(binding.source, raw.replace("２", "2"))


def test_same_leaf_retains_explicit_and_resolved_omitted_occurrences() -> None:
    definition, binding, _, _ = binding_sample()
    data = binding.model_dump(mode="json")
    explicit = object_value(array(data["occurrences"])[0])
    data["occurrences"] = [
        explicit,
        {
            "slot": "n",
            "ordinal": 1,
            "raw_spans": [],
            "canonical_spans": [],
            "source_unit": None,
            "source_presence": "omitted",
            "resolution_rule": "fixture.antecedent.v1",
        },
    ]
    mixed = SourceBinding.model_validate_json(canonical(data))
    mixed = mixed.model_copy(update={"id": "bind:" + hash_payload(mixed.payload())})
    mixed.verify(definition)
    assert mixed.values == {"n": 2}
    assert [o.source_presence for o in mixed.occurrences] == ["explicit", "omitted"]
    assert (
        mixed.occurrences[0].canonical_spans
        == definition.leaf_schema.slots[0].occurrences
    )

    data = mixed.model_dump(mode="json")
    object_value(array(data["occurrences"])[1])["resolution_rule"] = None
    unknown = SourceBinding.model_validate_json(canonical(data))
    with pytest.raises(ValueError, match="named resolution rule"):
        unknown.verify(definition)


def test_binding_rejects_missing_leaf_value_and_lost_occurrence() -> None:
    definition, binding, _, _ = binding_sample()
    mutations: tuple[tuple[dict[str, JsonValue], str], ...] = (
        ({"/values": {}}, "missing required"),
        ({"/occurrences": []}, "canonical positions"),
        ({"/occurrences/0/ordinal": 1}, "ordinals must"),
    )
    for changes, message in mutations:
        value = SourceBinding.model_validate_json(
            canonical(changed(binding.model_dump(mode="json"), changes))
        )
        with pytest.raises(ValueError, match=message):
            value.verify(definition)


@pytest.mark.parametrize(
    ("field", "ordinal"), [("effect", 1), ("section", None), ("label", None)]
)
def test_source_owner_field_and_ordinal_are_closed(
    field: str, ordinal: int | None
) -> None:
    _, binding, _, _ = binding_sample()
    data = changed(
        binding.source.model_dump(mode="json"), {"/field": field, "/ordinal": ordinal}
    )
    with pytest.raises(ValidationError, match=r"ordinal disagree|not legal"):
        SourceDescriptor.model_validate_json(canonical(data))
