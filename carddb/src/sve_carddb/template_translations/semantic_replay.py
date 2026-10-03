"""Replay a v2 inventory through fixed installed semantics and compare all six streams."""

from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build_inputs import BuildContext, SourceUse, uses_sorted
from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.snapshot.values import array, canonical, digest, object_value, parse
from sve_carddb.template_parameter_rules.loader import load_config
from sve_carddb.template_parameter_rules.models import LEGACY_IDS
from sve_carddb.template_parameter_rules.replay import (
    JP_BATCH,
    _references,
    _vocabulary,
)
from sve_carddb.template_semantics.environment import capture, differences
from sve_carddb.template_semantics.registry import ROOT, verify, verify_recipes
from sve_carddb.template_semantics.v1.candidates import build
from sve_carddb.template_semantics.v1.checkpoint import compare, parse_legacy
from sve_carddb.template_semantics.v1.flavor import reconstruct as flavor_replay
from sve_carddb.template_semantics.v1.inventory import coverage, fields, scan_batch
from sve_carddb.template_semantics.v1.members import _members
from sve_carddb.template_semantics.v1.output import Output, summarize
from sve_carddb.template_semantics.v1.parameters.references import References
from sve_carddb.template_semantics.v1.projection import project
from sve_carddb.template_semantics.v1.resolve import (
    _historical_positions,
    _resolve,
    numeric_identity,
)
from sve_carddb.template_sources.flavor import WHITE_SPACE
from sve_carddb.template_sources.normalizer import VERSION
from sve_carddb.template_translations.flavor_models import FlavorInputs
from sve_carddb.template_translations.replay_models import FlavorReplayInputs
from sve_carddb.template_translations.sources import SourceReplay
from sve_carddb.translations.sources import CODE_PATH, RUNTIME

if TYPE_CHECKING:
    from sve_carddb.catalog.adoption_sources import PinnedRepository
    from sve_carddb.template_semantics.registry import Checked
    from sve_carddb.template_sources.models import Recipe
    from sve_carddb.template_translations.replay_models import ReplayContext
    from sve_carddb.template_translations.sources import TemplateSources

PARAMETERS = "template-parameters-jp-candidate-v1"
PARSER = "translation-jp-v1"


def evidence_context(repository: PinnedRepository, producer: str) -> BuildContext:
    """Audit producer evidence bytes without pretending they are executing code."""
    raw = repository.read_many(producer, RUNTIME)
    return BuildContext.from_inputs(
        producer,
        raw,
        {
            "translation_recipes": {
                "translation-" + p + "-v1": {
                    "version": "translation-" + p + "-v1",
                    "program_revision": producer,
                    "code_path": CODE_PATH,
                    "code_hash": digest(raw[CODE_PATH]),
                    "config": {"provider": p},
                    "config_hash": digest(canonical({"provider": p})),
                }
                for p in ("jp", "en", "sv1", "svwb")
            }
        },
    )


def _field_state(text: str | None) -> str:
    if text is None:
        return "unknown"
    if not text:
        return "empty"
    return "whitespace_only" if all(ord(c) in WHITE_SPACE for c in text) else "present"


def batch_outputs(
    frozen: FrozenSources, *, flavor: bool
) -> tuple[list[JsonValue], dict[tuple[str, str], str], tuple[SourceUse, ...]]:
    """Read the independently sealed complete set, retaining historical raw and absent fields."""
    proofs: list[JsonValue] = []
    texts: dict[tuple[str, str], str] = {}
    uses = []
    current_ids = {item.source_version_id for item in frozen.inventory.current}
    for version in sorted(frozen.entries):
        source, raw, _ = frozen.read(version, parser_version=PARSER)
        uses.append(
            SourceUse(source=source, usage="template_frozen_batch", locator="/")
        )
        if version not in current_ids:
            continue
        _, document = project(raw, source.url, "jp")
        projection_hash = digest(canonical(document))
        selected = (
            tuple(
                (f"/faces/{i}/flavor", object_value(face).get("flavor"), None)
                for i, face in enumerate(array(object_value(document)["faces"]))
            )
            if flavor
            else fields(document)
        )
        for locator, text, _ in selected:
            if text is not None and not isinstance(text, str):
                raise ValueError("Semantic field must be exact text or unknown")
            proofs.append(
                {
                    "source_version_id": version,
                    "locator": locator,
                    "complete_projection_hash": projection_hash,
                    "state": _field_state(text),
                    "text_hash": None if text is None else digest(text.encode()),
                }
            )
            if text is not None:
                texts[version, locator] = text
            uses.append(
                SourceUse(source=source, usage="template_field", locator=locator)
            )
    return proofs, texts, uses_sorted(uses)


def effect(  # ruff: ignore[too-many-locals,complex-structure] -- a full batch shares fixed semantics and one policy/reference closure
    sources: TemplateSources, pins: tuple[Recipe, ...], checked: Checked
) -> tuple[SourceReplay, list[JsonValue], tuple[SourceUse, ...], JsonValue]:
    """The original recognition policy still controls roles and full position replay."""
    by_id = {r.id: r for r in pins}
    if set(by_id) != {PARAMETERS, PARSER, VERSION}:
        raise ValueError(
            "Formal template inventory requires its three complete recipe pins"
        )
    recipe = by_id[PARAMETERS]
    source_pins = tuple(r for r in pins if r.id != PARAMETERS)
    if set(recipe.config) != {
        "recognition_policy",
        "source_recipes",
        "source_batch",
        "references",
        "legacy_file_hash",
    } or recipe.config["legacy_file_hash"] != digest(sources.legacy_bytes):
        raise ValueError(
            "Formal template recipe requires its exact legacy input and closed config"
        )
    if canonical(recipe.config["source_recipes"]) != canonical(
        [r.model_dump(mode="json") for r in source_pins]
    ):
        raise ValueError(
            "Formal template source recipes differ from the parameter recipe"
        )
    if by_id[VERSION].config or by_id[PARSER].config != {"provider": "jp"}:
        raise ValueError("Fixed effect source recipes require their closed configs")
    loaded = load_config(
        recipe.config, sources.repository, main_revision=sources.main_revision
    )
    batch = object_value(recipe.config["source_batch"])
    if set(batch) != {"store_id", "batch_id"} or not all(
        isinstance(v, str) for v in batch.values()
    ):
        raise ValueError(
            "Formal template recipe requires one exact frozen source batch"
        )
    store, identifier = str(batch["store_id"]), str(batch["batch_id"])
    if store not in sources.stores:
        raise ValueError("Formal template frozen source store is unavailable")
    if loaded is not None and canonical(batch) != canonical(
        loaded.policy.scope.source_batches[0].model_dump(mode="json")
    ):
        raise ValueError("Formal template batch differs from its recognition scope")
    frozen = FrozenSources(sources.stores[store], store, identifier)
    scan = scan_batch(frozen, pins=source_pins)
    legacy = parse_legacy(sources.legacy_bytes)
    checkpoint = compare(scan, legacy)
    if (
        any(
            object_value(checkpoint[k])["complete"] is not True
            for k in ("fingerprints", "legacy_member_coverage")
        )
        or object_value(checkpoint["fingerprints"])["additional_ids"]
        or object_value(checkpoint["legacy_member_coverage"])["additional_members"]
    ):
        raise ValueError(
            "Formal template replay must reproduce all legacy fingerprints and uses"
        )
    reference_uses: list[SourceUse] = []
    old_refs = _references(
        sources.repository,
        recipe,
        sources.stores,
        semantics=checked,
        evidence_uses=reference_uses,
    )
    _vocabulary(old_refs, recipe, sources.proposals)
    refs = References(
        card_names=old_refs.card_names,
        terms=old_refs.terms,
        vocabulary=old_refs.vocabulary,
        pins=old_refs.pins,
    )
    enabled = (
        ()
        if loaded is None
        else tuple(
            sorted(
                r.rule_id for r in loaded.policy.rules if r.rule_id not in LEGACY_IDS
            )
        )
    )
    candidates = build(frozen, scan, refs, enabled_rules=enabled)
    numeric = tuple(
        sorted(
            numeric_identity(c, h, h.numeric_rule)
            for c in candidates.entries
            for h in c.slots
            if h.numeric_rule is not None
        )
    )
    if frozen.batch_id == JP_BATCH and (
        len(numeric),
        object_value(checkpoint["fingerprints"])["expected"],
        object_value(checkpoint["legacy_member_coverage"])["expected"],
    ) != (14782, 3669, 13913):
        raise ValueError(
            "Formal template first JP batch differs from its fixed baseline counts"
        )
    if (
        loaded is not None
        and loaded.historical_revision is not None
        and _historical_positions(frozen, scan, candidates) != numeric
    ):
        raise ValueError(
            "Formal template recognition changed an original numeric position"
        )
    resolved, remaining = _resolve(loaded, candidates)
    solved = {
        (str(r["inventory_id"]), str(r["slot"])): r
        for r in (object_value(parse(b)) for b in resolved)
    }
    pending: dict[str, set[str]] = {}
    for raw in remaining:
        row = object_value(parse(raw))
        pending.setdefault(str(row["inventory_id"]), set()).update(
            str(i) for i in array(row["issues"])
        )
    proofs, texts, raw_uses = batch_outputs(frozen, flavor=False)
    members = _members(
        tuple(scan.entries), tuple(candidates.entries), texts, solved, pending
    )
    enriched: JsonValue = {
        **checkpoint,
        "numeric_positions": [parse(b) for b in numeric],
        "fingerprints": [
            [o.template, o.member_hash, digest(o.normalized.encode())]
            for o in sorted(
                scan.occurrences, key=lambda o: (o.template, o.member_hash, o.entry_id)
            )
        ],
        "resolved_slots": [parse(b) for b in resolved],
        "remaining_slots": [parse(b) for b in remaining],
    }
    replay = SourceReplay(
        canonical(recipe.model_dump(mode="json")),
        members,
        canonical(
            {
                **coverage(scan),
                "pages": list[JsonValue](scan.pages),
                "field_proofs": list[JsonValue](scan.fields),
            }
        ),
        canonical(enriched),
    )
    provenance: JsonValue = {
        "references": refs.pins,
        "reference_inputs": recipe.config["references"],
        "policy_pin": None if loaded is None else loaded.pin.model_dump(mode="json"),
        "policy_dependencies": []
        if loaded is None
        else [list(p) for p in loaded.dependencies],
    }
    return replay, proofs, uses_sorted((*raw_uses, *reference_uses)), provenance


def _compute(  # ruff: ignore[too-many-locals] -- streams and provenance remain independently verifiable
    sources: TemplateSources,
    pins: tuple[Recipe, ...],
    context: ReplayContext,
) -> SourceReplay:
    """Expected is supplied from immutable input; generation must independently reload it."""
    flavor = isinstance(context.inputs, FlavorReplayInputs)
    checked = verify(
        sources.repository,
        context.semantic_bindings,
        flavor=flavor,
        environment=context.environment,
    )
    verify_recipes(sources.repository, pins, checked)
    extra: JsonValue = {}
    if flavor:
        if {r.id for r in pins} != {"flavor-exact-v1", PARSER} or any(
            r.config != ({} if r.id == "flavor-exact-v1" else {"provider": "jp"})
            for r in pins
        ):
            raise ValueError(
                "Flavor requires its exact independent recipe and parser pins"
            )
        inputs = FlavorInputs.model_validate_json(
            canonical(context.inputs.model_dump(mode="json", exclude={"kind"}))
        )
        replay = flavor_replay(
            sources.repository,
            sources.stores,
            pins,
            inputs,
            sources.main_revision,
            semantics=checked,
            build_context=evidence_context(sources.repository, pins[0].code_revision),
        )
        frozen = FrozenSources(
            sources.stores[inputs.source_batch.store_id],
            inputs.source_batch.store_id,
            inputs.source_batch.batch_id,
        )
        fields_out, _, raw_uses = batch_outputs(frozen, flavor=True)
        coverage_out = object_value(parse(replay.source_coverage))
        host_data = {
            name: coverage_out.pop(name)
            for name in ("runtime_dependencies", "runtime_configuration")
        }
        identity_uses = tuple(
            SourceUse.model_validate(row)
            for row in array(coverage_out["identity_source_uses"])
        )
        uses = uses_sorted((*raw_uses, *identity_uses))
        extra = {"identity_files": coverage_out["identity_files"]}
    else:
        replay, fields_out, uses, extra = effect(sources, pins, checked)
        coverage_out = object_value(parse(replay.source_coverage))
        host_data = {}
    use_records: list[JsonValue] = [
        {
            "kind": "raw",
            "semantic_parser": {
                "id": checked.bindings[0].id,
                "hash": checked.bindings[0].hash,
            },
            **u.model_dump(mode="json"),
        }
        for u in uses
    ]
    use_records.append({"kind": "authored_dependencies", "value": extra})
    output = summarize(
        replay.entries, fields_out, coverage_out, parse(replay.checkpoint), use_records
    )
    actual_environment = capture(ROOT, producer=False)
    delta = differences(context.environment, actual_environment)
    provenance: JsonValue = {
        "producer_pins": [list(p) for p in checked.exact_pins],
        "current_environment": actual_environment.model_dump(mode="json"),
        "environment_differences": delta,
        "producer_runtime": host_data,
        "actual_outputs": output.manifest.model_dump(mode="json"),
    }
    return SourceReplay(
        replay.recipe,
        replay.entries,
        canonical(coverage_out),
        replay.checkpoint,
        output,
        canonical(provenance),
        uses,
    )


def compare_outputs(
    output: Output,
    context: ReplayContext,
    *,
    delta: dict[str, JsonValue],
) -> None:
    """Never quietly replace the original expected hashes with newly computed ones."""
    expected = context.expected_outputs.streams.model_dump(mode="json")
    actual = output.manifest.streams.model_dump(mode="json")
    for name in expected:
        if expected[name] != actual[name]:
            detail = canonical(
                {
                    "stream": name,
                    "expected": expected[name],
                    "actual": actual[name],
                    "environment_differences": delta,
                }
            ).decode()
            raise ValueError("replay_output_mismatch: " + detail)


def reconstruct(
    sources: TemplateSources, pins: tuple[Recipe, ...], context: ReplayContext
) -> SourceReplay:
    """Formal consumption always compares the separately loaded immutable baseline."""
    result = _compute(sources, pins, context)
    assert result.semantic_output is not None
    delta = object_value(
        object_value(parse(result.provenance))["environment_differences"]
    )
    compare_outputs(result.semantic_output, context, delta=delta)
    return result


def produce(
    sources: TemplateSources, pins: tuple[Recipe, ...], context: ReplayContext
) -> tuple[ReplayContext, SourceReplay]:
    """First generation is a candidate; an independent loader must replay its written baseline."""
    result = _compute(sources, pins, context)
    assert result.semantic_output is not None
    return context.model_copy(
        update={"expected_outputs": result.semantic_output.manifest}
    ), result
