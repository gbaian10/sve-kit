"""Create first candidates from immutable inputs; generation never asserts adoption."""

from typing import TYPE_CHECKING

from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.template_semantics.environment import capture
from sve_carddb.template_semantics.registry import ROOT, bindings, installed, regular
from sve_carddb.template_semantics.versions import RECIPES
from sve_carddb.template_sources.models import Recipe
from sve_carddb.template_translations.replay_models import (
    EffectInputs,
    ExpectedOutputs,
    FlavorReplayInputs,
    ReplayContext,
)
from sve_carddb.template_translations.semantic_replay import produce

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.catalog.adoption_sources import PinnedRepository
    from sve_carddb.template_translations.sources import SourceReplay, TemplateSources

PARAMETERS = "template-parameters-jp-candidate-v1"
NORMALIZER = "classification-jp-v0-v1"
PARSER = "translation-jp-v1"


def empty_outputs() -> ExpectedOutputs:
    """The private generation placeholder is never a formal replay success."""
    wire: dict[str, JsonValue] = {
        "format": 1,
        "recipe": "semantic-output-v1",
        "streams": {
            name: {"count": 0, "hash": digest(canonical([]))}
            for name in (
                "entries",
                "fields",
                "members",
                "coverage",
                "checkpoint",
                "source_uses",
            )
        },
    }
    wire["root"] = digest(canonical(wire))
    return ExpectedOutputs.model_validate(wire)


def recipe(
    repository: PinnedRepository,
    revision: str,
    identifier: str,
    config: dict[str, JsonValue],
) -> Recipe:
    """The caller provides business inputs; code path selection stays in the finite registry."""
    path = RECIPES[identifier]
    raw = regular(repository, revision, (path,))[path]
    if raw != installed(path):
        raise ValueError(
            "Template producer entrypoint differs from installed fixed code"
        )
    return Recipe(
        id=identifier,
        code_revision=revision,
        code_path=path,
        code_hash=digest(raw),
        config=config,
        config_hash=digest(canonical(config)),
    )


def generate(
    sources: TemplateSources,
    producer: str,
    inputs: EffectInputs | FlavorReplayInputs,
    *,
    effect_config: dict[str, JsonValue] | None = None,
) -> tuple[tuple[Recipe, ...], ReplayContext, SourceReplay]:
    """An independent consumer must reload the subsequently written baseline in a fresh session."""
    flavor = isinstance(inputs, FlavorReplayInputs)
    fixed = bindings(sources.repository, producer, flavor=flavor)
    environment = capture(ROOT, producer=True)
    parser = recipe(sources.repository, producer, PARSER, {"provider": "jp"})
    pins: tuple[Recipe, ...]
    if flavor:
        pins = (recipe(sources.repository, producer, "flavor-exact-v1", {}), parser)
    else:
        if effect_config is None or set(effect_config) != {
            "recognition_policy",
            "source_batch",
            "references",
        }:
            raise ValueError(
                "Template effect generation requires its closed business inputs"
            )
        source_pins = tuple(
            sorted(
                (parser, recipe(sources.repository, producer, NORMALIZER, {})),
                key=lambda r: r.id,
            )
        )
        config: dict[str, JsonValue] = {
            **effect_config,
            "source_recipes": [r.model_dump(mode="json") for r in source_pins],
            "legacy_file_hash": digest(sources.legacy_bytes),
        }
        pins = tuple(
            sorted(
                (
                    *source_pins,
                    recipe(sources.repository, producer, PARAMETERS, config),
                ),
                key=lambda r: r.id,
            )
        )
    context = ReplayContext(
        format=1,
        semantic_bindings=fixed,
        environment=environment,
        inputs=inputs,
        expected_outputs=empty_outputs(),
    )
    context, result = produce(sources, pins, context)
    return pins, context, result
