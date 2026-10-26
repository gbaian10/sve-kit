"""Canonical card and body anchor bases and explicit mismatch diagnostics."""

from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.core.json import array, canonical, digest, object_value, string
from sve_carddb.domains.effects.author_inputs import AuthorInputError, Diagnostic
from sve_carddb.domains.effects.author_materialization import (
    MaterializationContext,
    materialize_card,
)
from sve_carddb.domains.effects.author_sources import SourceError

if TYPE_CHECKING:
    from sve_carddb.domains.effects.author_current import CurrentAuthorSources
    from sve_carddb.domains.effects.author_expansion import AuthorExpander
    from sve_carddb.domains.effects.author_handwritten import Handwritten
    from sve_carddb.domains.effects.author_inputs import Stage

RECIPE = "effect-anchor-v1"


def fingerprint(value: JsonValue) -> str:
    """Keep the new outer recipe distinct from existing source and body recipes."""
    return RECIPE + ":" + digest(canonical({"recipe": RECIPE, "payload": value}))


def _recipes(expander: AuthorExpander) -> dict[str, JsonValue]:
    try:
        storage: dict[str, JsonValue] = {
            name: digest((expander.schema_directory / name).read_bytes())
            for name in (
                "author-storage.schema.json",
                "effect-body-storage.schema.json",
                "effect-anchors.schema.json",
            )
        }
    except OSError:
        raise AuthorInputError(
            Diagnostic("recipe", "missing_recipe_resource")
        ) from None
    return {
        "expansion": expander.expansion_recipe,
        "adapter": expander.adapter_recipe,
        "storage": storage,
    }


def body_anchor(
    body: dict[str, JsonValue], current: CurrentAuthorSources, expander: AuthorExpander
) -> dict[str, JsonValue]:
    """Hash the exact body interface independently of any witness card."""
    frame = current.registry.get(string(body["frame_id"]))
    if frame is None:
        raise SourceError("companion_registry_frame_missing")
    value = {
        key: body[key]
        for key in ("frame_id", "ability_scope", "body_version", "params", "ports")
    }
    value["registrations"] = [
        {key: object_value(registration)[key] for key in ("id", "domain")}
        for registration in array(body["registrations"])
    ]
    return {
        **value,
        "basis": fingerprint(
            {
                **value,
                "interface_key": frame.projection.interface_key(frame.id),
                "recipes": _recipes(expander),
            }
        ),
    }


def compare_card(
    old: dict[str, JsonValue], new: dict[str, JsonValue], number: str
) -> Diagnostic | None:
    """Report only stages justified by the exact committed fields."""
    for key in ("card_id", "source"):
        if old.get(key) != new[key]:
            return Diagnostic("source", "stale_anchor", card_no=number)
    old_uses, new_uses = array(old["uses"]), array(new["uses"])
    if len(old_uses) != len(new_uses):
        return Diagnostic("anchor", "anchor_use_closure", card_no=number)
    for index, (before, after) in enumerate(zip(old_uses, new_uses, strict=True)):
        a, b = object_value(before), object_value(after)
        stages: tuple[tuple[tuple[str, ...], Stage], ...] = (
            (
                (
                    "line",
                    "face",
                    "section",
                    "scope",
                    "face_id",
                    "binding_id",
                    "occurrence",
                ),
                "binding",
            ),
            (("frame_id", "ability_scope"), "frame"),
            (("body", "edition", "body_version", "registration"), "body"),
            (("params",), "params"),
            (("ports", "imports", "keywords"), "dependency"),
        )
        for keys, stage in stages:
            if any(a.get(key) != b.get(key) for key in keys):
                return Diagnostic(
                    stage,
                    "stale_anchor",
                    ("cards", number, "abilities", index),
                    card_no=number,
                    card_line=b["line"] if type(b["line"]) is int else None,
                )
    if old != new:
        return Diagnostic("anchor", "stale_anchor", card_no=number)
    return None


def build_card(  # ruff: ignore[too-many-locals] -- compute the complete card basis from independently checked current sources
    current: CurrentAuthorSources,
    number: str,
    handwritten: Handwritten,
    container: str,
    expander: AuthorExpander,
    legacy_bodies: dict[str, dict[str, JsonValue]],
) -> tuple[dict[str, JsonValue], dict[str, JsonValue]]:
    """Rebuild exact uses and their full current basis without trusting the old pin."""
    payload = current.card_payload(current.printing_card(number), inspect=True)
    context = MaterializationContext(current, expander.schema_directory, payload)
    card_id, card, definitions, locations = materialize_card(
        context, number, handwritten, container=container
    )
    sources = context.sources
    uses: list[JsonValue] = []
    definitions_iter = iter(definitions)
    by_index = {index: path for path, index in locations.items()}
    for index, raw in enumerate(array(handwritten.cards[number]["abilities"])):
        value = object_value(raw)
        path = by_index[index]
        ability_node: JsonValue = card
        for part in path[2:]:
            ability_node = (
                object_value(ability_node)[part]
                if isinstance(part, str)
                else array(ability_node)[part]
            )
        ability = object_value(ability_node)
        source = object_value(array(ability["sources"])[0])
        source_owner = object_value(object_value(source["occurrence"])["owner"])
        frame = next(s.frame for s in sources if s.binding.id == source["binding_id"])
        use: dict[str, JsonValue] = {
            key: value[key]
            for key in ("line", "face", "section", "scope")
            if key in value
        }
        use.update(
            {
                "face_id": payload.revisions[string(source_owner["revision_id"])],
                **source,
                "frame_id": frame.id,
            }
        )
        if "use" in value:
            body, edition = next(definitions_iter)
            key = f"{value['use']}/{edition}"
            if key in legacy_bodies and legacy_bodies[key] != body:
                raise SourceError("body_source_frame_mismatch")
            legacy_bodies[key] = body
            generated = object_value(ability["generated_by"])
            use.update(
                {
                    "body": value["use"],
                    "edition": edition,
                    "body_version": body["body_version"],
                    "registration": object_value(generated["macro"])[
                        "applicability_scope"
                    ],
                    "params": generated["params"],
                }
            )
        else:
            for key in ("ports", "imports", "keywords"):
                if key in ability:
                    use[key] = ability[key]
        uses.append(use)
    card_anchor: dict[str, JsonValue] = {
        "card_id": card_id,
        "source": object_value(card["meta"])["source"],
        "uses": uses,
        "legacy_shape": {"container": container},
    }
    body_pins: list[JsonValue] = [
        body_anchor(body, current, expander) for body, _ in definitions
    ]
    card_anchor["basis"] = fingerprint(
        {
            **card_anchor,
            "source_payload": payload.payload.model_dump(mode="json"),
            "sources": [
                {
                    "frame": s.frame.model_dump(mode="json"),
                    "binding": s.binding.model_dump(mode="json"),
                    "canonical_source": s.canonical_source,
                    "interface_key": s.frame.projection.interface_key(s.frame.id),
                }
                for s in sources
            ],
            "bodies": body_pins,
            "declarations": [
                {
                    key: value
                    for key, value in object_value(a).items()
                    if key != "sources"
                }
                for a in abilities(card)
            ],
            "rulings": object_value(card["meta"])["rulings"],
            "deck_rule": card.get("deck_rule"),
            "recipes": _recipes(expander),
        }
    )
    return card, card_anchor


def abilities(card: dict[str, JsonValue]) -> list[JsonValue]:
    """Flatten semantic containers in their preserved ability order."""
    if "abilities" in card:
        return array(card["abilities"])
    return [
        a
        for face in object_value(card["faces"]).values()
        for a in array(object_value(face)["abilities"])
    ]
