"""Sealed entry provenance and play-origin conditions for explicit N2 preparation."""

import re
from dataclasses import dataclass, field, replace
from typing import TYPE_CHECKING

from sve_carddb.contracts.four_layer import (
    EventRef,
    FaceRevisionOwner,
    GlossaryReference,
    ObjectGeneration,
    ObjectSetRef,
    OccurrenceKey,
    Port,
    PredicateExpr,
    PrintingFaceOwner,
    Span,
    hash_payload,
)
from sve_carddb.domains.translations.four_layer_context import (
    DEFAULT_REGISTRY,
    Descriptor,
    ImportSignature,
    Parameter,
)
from sve_carddb.domains.translations.four_layer_event_preparation import (
    context_port,
    interface,
    occurrence,
    source_scope,
    update_source,
)
from sve_carddb.domains.translations.four_layer_n1_leaves import Leaf, leaf
from sve_carddb.domains.translations.four_layer_normalizer import SourceField
from sve_carddb.domains.translations.four_layer_preparation import PreparationFamily

if TYPE_CHECKING:
    from sve_carddb.domains.translations.four_layer_classification import Classifier
    from sve_carddb.domains.translations.four_layer_normalizer import SourcePart
    from sve_carddb.domains.translations.four_layer_preparation import PreparedField
    from sve_carddb.domains.translations.four_layer_semantics import CardContext

_ENTRY = re.compile(
    r"(?P<subject>これ|それ)が(?:(?P<zone>手札|墓場)(?P<ne>以外)?から|"
    r"(?:(?P<trait>ウマ娘)・カードの)?能力によって)場に出ていたなら"
)
_PLAY = re.compile(
    r"これ(?:が(?P<ex>EXエリア)からプレイされていた|を(?P<hand>手札)からプレイしていた)なら"
)
_PLAY_RULE = Descriptor(
    "play.self.from_zone.eq.v1",
    "predicate.play.provenance.v1",
    "played_from_condition",
    "Bool",
    "truth",
    (
        Parameter(
            "zone",
            "ZoneSet",
            "played_from_zone",
            "zone.single.v1",
            (("ex",), ("hand",)),
        ),
    ),
    (
        ImportSignature("play_subject", "ObjectSet", "at_event", "one", "holder"),
        ImportSignature(
            "play_fact",
            "EventRef",
            "at_event",
            event_kind="Play",
            subject_import="play_subject",
        ),
    ),
    "played_from",
    ("E.play", "V.event-field", "played_from"),
)
DESCRIPTORS = (_PLAY_RULE,)


@dataclass
class _ProvenanceSources:
    raw: str
    part: SourcePart
    classifier: Classifier
    source: OccurrenceKey
    additions: list[Leaf] = field(default_factory=list)
    ports: list[Port] = field(default_factory=list)
    scopes: list[str] = field(default_factory=list)
    issues: set[str] = field(default_factory=set)

    def imports(
        self, scope: str, prefix: str, *, play: bool, event_subject: bool, trait: bool
    ) -> dict[str, str]:
        subject_name, fact_name = prefix + "_subject", prefix + "_fact"
        # Source handles request runtime evidence; they do not assert an event occurred.
        subject = ObjectGeneration(
            object_id="source-object:"
            + hash_payload(
                {
                    "source": self.source.model_dump(mode="json"),
                    "scope": scope,
                    "origin": "event_subject" if event_subject else "holder",
                }
            ),
            generation=0,
        )
        direct = (
            ObjectGeneration(
                object_id="source-ability:"
                + hash_payload(self.source.model_dump(mode="json")),
                generation=0,
            )
            if trait
            else None
        )
        self.ports.extend(
            (
                context_port(
                    subject_name,
                    "play_subject" if play else "entry_subject",
                    scope,
                    ObjectSetRef(
                        kind="ObjectSet",
                        objects=(subject,),
                        origin="event_subject" if event_subject else "holder",
                    ),
                    "at_event",
                    self.source,
                ),
                context_port(
                    fact_name,
                    "play_fact" if play else "entry_fact",
                    scope,
                    EventRef(
                        kind="EventRef",
                        event_kind="Play" if play else "EnterField",
                        event_id="source-event:"
                        + hash_payload(
                            {
                                "source": self.source.model_dump(mode="json"),
                                "scope": scope,
                                "kind": "Play" if play else "EnterField",
                            }
                        ),
                        subject=subject,
                        direct_ability_source=direct,
                    ),
                    "at_event",
                    self.source,
                ),
            )
        )
        imports = {
            "play_subject" if play else "entry_subject": subject_name,
            "play_fact" if play else "entry_fact": fact_name,
        }
        if direct is not None:
            imports["ability_source"] = prefix + "_ability_source"
            self.ports.append(
                context_port(
                    imports["ability_source"],
                    "entry_ability_source",
                    scope,
                    ObjectSetRef(
                        kind="ObjectSet",
                        objects=(direct,),
                        origin="direct_ability_source",
                    ),
                    "at_event",
                    self.source,
                )
            )
        return imports

    def operand(self, match: re.Match[str], prefix: str, *, play: bool) -> Leaf | None:
        if play or match["zone"]:
            group = "ex" if play and match["ex"] else "hand" if play else "zone"
            zone = (
                "ex"
                if group == "ex"
                else "hand"
                if group == "hand" or match[group] == "手札"
                else "graveyard"
            )
            item = leaf(
                "ZoneSet",
                "played_from_zone" if play else "entry_from_zone",
                "zone.single.v1",
                (zone,),
                ((Span(start=match.start(group), end=match.end(group)),),),
                "N2.played_from" if play else "N2.A11",
            )
        elif match["trait"]:
            term = self.classifier.terms.get("term:trait.umamusume")
            if term is None or term.category != "trait":
                return None
            item = leaf(
                "Concept",
                "entry_source_trait",
                "concept.trait.v1",
                GlossaryReference(kind="glossary", key=term.id),
                ((Span(start=match.start("trait"), end=match.end("trait")),),),
                "N2.A11",
            )
        else:
            return None
        return replace(
            item, slot=item.slot.model_copy(update={"name": prefix + "_operand"})
        )

    def condition(self, match: re.Match[str], prefix: str) -> None:
        play = match.re is _PLAY
        event_subject = not play and match["subject"] == "それ"
        if not play and event_subject and (match["zone"] != "墓場" or match["ne"]):
            return
        if not play and match["ne"] and match["zone"] != "手札":
            return
        operand = self.operand(match, prefix, play=play)
        if not play and match["trait"] and operand is None:
            return
        scope = source_scope(
            self.part.canonical_source,
            match.start(),
            match.end(),
            _consumer(self.part.canonical_source, match, event_subject=event_subject),
        )
        self.scopes.append(scope)
        imports = self.imports(
            scope,
            prefix,
            play=play,
            event_subject=event_subject,
            trait=not play and bool(match["trait"]),
        )
        if (
            event_subject
            and self.part.canonical_source[: match.start()]
            != "自分の場にフォロワーが出たとき、"
        ):
            self.issues.add("missing_unique_entry_event_provider")
            del self.ports[-2:]
        identifier = _identifier(match, play=play, event_subject=event_subject)
        expr = PredicateExpr(
            kind="predicate",
            expression_id=identifier,
            leaves=(operand.slot.name,) if operand else (),
            imports=imports,
            scope=scope,
        )
        rule = _PLAY_RULE if play else DEFAULT_REGISTRY.definition(expr)
        item = leaf(
            "PredicateExpr",
            rule.role,
            rule.domain,
            expr,
            ((Span(start=match.start(), end=match.start() + 2),),),
            "N2.played_from" if play else "N2.A11",
        )
        if operand:
            self.additions.append(operand)
        self.additions.append(
            replace(
                item,
                slot=item.slot.model_copy(update={"name": prefix, "expression": expr}),
                abstract=False,
            )
        )


def _consumer(text: str, match: re.Match[str], *, event_subject: bool) -> str:
    if "下記から" in text[: match.start()] and text[match.end() :].startswith(
        "、代わりに"
    ):
        return "mode_count"
    if text[: match.start()].endswith("この能力は") and text[match.end() :].startswith(
        "使える"
    ):
        return "ability_availability"
    return "event_body" if event_subject else "body_condition"


def _identifier(match: re.Match[str], *, play: bool, event_subject: bool) -> str:
    if play:
        return _PLAY_RULE.id
    if event_subject:
        return "entry.event_subject.from_zone.eq.v1"
    if match["zone"]:
        return (
            "entry.self.from_zone.ne.v1"
            if match["ne"]
            else "entry.self.from_zone.eq.v1"
        )
    return (
        "entry.self.method.ability_source_trait.v1"
        if match["trait"]
        else "entry.self.method.ability.v1"
    )


def prepare_provenance(
    raw: str,
    prepared: PreparedField,
    classifier: Classifier,
    context: CardContext | None,
) -> PreparedField:
    """Keep single-event entry facts separate from CurrentTurn and Play queries."""
    del context
    derived = prepared.derived
    if not isinstance(
        derived.field.source.owner, (FaceRevisionOwner, PrintingFaceOwner)
    ) or derived.field.source.field not in {"effect", "section"}:
        return prepared
    parts, recognized, rows, interfaces = (
        list(derived.field.parts),
        list(derived.recognized),
        list(derived.registry_rows),
        list(prepared.interfaces),
    )
    for index, original in enumerate(derived.original.parts):
        if original.source_span.role != "body":
            continue
        found = _ProvenanceSources(
            raw, original, classifier, occurrence(derived.field.source, original)
        )
        for number, match in enumerate(
            sorted(
                (
                    *_ENTRY.finditer(original.canonical_source),
                    *_PLAY.finditer(original.canonical_source),
                ),
                key=lambda m: m.start(),
            )
        ):
            found.condition(match, f"p2_provenance_{number}")
        if found.additions:
            parts[index], recognized[index], rows[index], _ = update_source(
                raw,
                original,
                parts[index],
                recognized[index],
                found.additions,
                classifier,
            )
            if found.issues:
                recognized[index] = replace(
                    recognized[index],
                    issues=tuple(sorted({*recognized[index].issues, *found.issues})),
                )
            interfaces[index] = interface(
                interfaces[index], tuple(found.scopes), tuple(found.ports)
            )
    return replace(
        prepared,
        derived=replace(
            derived,
            field=SourceField(derived.field.source, tuple(parts)),
            recognized=tuple(recognized),
            registry_rows=tuple(rows),
        ),
        interfaces=tuple(interfaces),
    )


PROVENANCE_FAMILY = PreparationFamily(
    "entry_and_play_provenance", DESCRIPTORS, prepare_provenance
)
