"""Finite CurrentTurn history sources, distinct from future triggers and entry facts."""

import re
from dataclasses import dataclass, field, replace
from typing import TYPE_CHECKING, Literal

from sve_carddb.contracts.four_layer import (
    CardNameReference,
    ContextExpression,
    Domain,
    FaceRevisionOwner,
    GlossaryReference,
    LeafSlot,
    ObjectGeneration,
    ObjectSetRef,
    OccurrenceKey,
    PlayerRef,
    Port,
    PredicateExpr,
    PrintingFaceOwner,
    Span,
    hash_payload,
)
from sve_carddb.contracts.n0 import SAFE_INTEGER
from sve_carddb.domains.translations.four_layer_ability import (
    ABILITY_HEAD,
    OPTION_LABEL,
)
from sve_carddb.domains.translations.four_layer_context import (
    DEFAULT_REGISTRY,
    Descriptor,
    HistorySignature,
    ImportSignature,
    Parameter,
)
from sve_carddb.domains.translations.four_layer_derivation import SourceFamily
from sve_carddb.domains.translations.four_layer_event_preparation import (
    context_port,
    interface,
    occurrence,
    source_scope,
    spelling,
    update_source,
)
from sve_carddb.domains.translations.four_layer_n1_leaves import Leaf, leaf
from sve_carddb.domains.translations.four_layer_normalizer import SourceField

if TYPE_CHECKING:
    from sve_carddb.contracts.four_layer import LeafType
    from sve_carddb.contracts.source_binding import TypedValue
    from sve_carddb.domains.translations.four_layer_classification import Classifier
    from sve_carddb.domains.translations.four_layer_derivation import DerivedField
    from sve_carddb.domains.translations.four_layer_normalizer import SourcePart
    from sve_carddb.domains.translations.four_layer_semantics import CardContext

_TURN = r"このターン(?:中に|、)"
_OWNER = r"(?P<owner>自分|相手)"
_TRAIT = r"(?P<trait>[^『』「」、。{}・]+)・"
_NUMBER = r"(?P<threshold>N)"
_PARAMETERS = {
    "owner": Parameter("owner", "Player", "history_owner", "player.relative.v1"),
    "trait": Parameter("trait", "Concept", "history_trait", "concept.trait.v1"),
    "trait_right": Parameter(
        "trait_right", "Concept", "history_trait", "concept.trait.v1"
    ),
    "name": Parameter("name", "CardName", "history_name", "card_name.any.v1"),
}


@dataclass(frozen=True)
class HistoryForm:
    key: str
    pattern: str
    event: str
    aggregate: Literal["count_facts", "count_subjects", "sum", "exists"]
    measure: str
    operands: tuple[str, ...] = ("owner",)
    include_execution: bool = True

    @property
    def descriptor(self) -> Descriptor:
        """Keep event, aggregate, filter shape and cutoff in a closed signature."""
        predicate = self.aggregate == "exists"
        return Descriptor(
            "history." + self.key + ".v1",
            "predicate.history.current_turn.v1"
            if predicate
            else "quantity.history.current_turn.v1",
            "history_condition" if predicate else "history_read_value",
            "Bool" if predicate else "Nat",
            "truth" if predicate else self.measure,
            tuple(_PARAMETERS[p] for p in self.operands),
            self.import_signature,
            "A2",
            ("H.query", "H.window", "H.fact", "X.predicate"),
            scalable=not predicate,
            history=HistorySignature(
                self.event, self.aggregate, include_execution=self.include_execution
            ),
        )

    @property
    def player_role(self) -> str:
        """Ownership of an affected object or resource does not identify its cause's actor."""
        if self.event.startswith("life_"):
            return "resource_owner"
        if self.event in {"play", "discard", "dice", "ub", "earth_rite"}:
            return "actor"
        return "subject_owner"

    @property
    def import_signature(self) -> tuple[ImportSignature, ...]:
        """A global query and a holder-restricted query have distinct context needs."""
        actor = (
            ()
            if self.event.startswith("self_buff")
            or (self.event == "field_to_graveyard" and not self.operands)
            else (ImportSignature(self.player_role, "PlayerRef", "current_turn"),)
        )
        subject = (
            (ImportSignature("subject", "ObjectSet", "current_turn", "one", "holder"),)
            if self.event.startswith("self_buff")
            or "exclude_holder" in self.key
            or "exclude_same_name" in self.key
            else ()
        )
        return (*actor, *subject)


_FORMS = (
    HistoryForm(
        "attack.follower.count_facts",
        _TURN
        + _OWNER
        + r"の(?:場の)?フォロワーが"
        + _NUMBER
        + r"回以上攻撃し(?:てい)?たなら",
        "attack",
        "count_facts",
        "event_count",
    ),
    HistoryForm(
        "attack.trait_follower.count_facts",
        _TURN
        + _OWNER
        + r"の場の"
        + _TRAIT
        + r"フォロワーが"
        + _NUMBER
        + r"回以上攻撃していたなら",
        "attack",
        "count_facts",
        "event_count",
        ("owner", "trait"),
    ),
    HistoryForm(
        "play.card.count_subjects",
        _TURN + _OWNER + r"がプレイしたカードの枚数",
        "play",
        "count_subjects",
        "card_count",
    ),
    HistoryForm(
        "play.card.exclude_execution.count_subjects",
        r"これを含めず、" + _TURN + _OWNER + r"がプレイしたカードの枚数",
        "play",
        "count_subjects",
        "card_count",
        include_execution=False,
    ),
    HistoryForm(
        "play.card.include_execution.count_subjects",
        _TURN + r"これを含めて" + _NUMBER + r"枚以上プレイしていたなら",
        "play",
        "count_subjects",
        "card_count",
        (),
    ),
    HistoryForm(
        "play.spell.exists",
        _TURN + _OWNER + r"がスペルをプレイしていたなら",
        "play",
        "exists",
        "truth",
    ),
    HistoryForm(
        "play.trait_card.exclude_holder.exists",
        _TURN + _OWNER + r"が他の" + _TRAIT + r"カードをプレイしていたなら",
        "play",
        "exists",
        "truth",
        ("owner", "trait"),
    ),
    HistoryForm(
        "play.name.exclude_holder.exists",
        _TURN + _OWNER + r"が他の『(?P<name>X)』をプレイしていたなら",
        "play",
        "exists",
        "truth",
        ("owner", "name"),
    ),
    HistoryForm(
        "play.two_traits.count_subjects",
        _TURN
        + _OWNER
        + r"がプレイした"
        + _TRAIT
        + r"カードや(?P<trait_right>[^『』「」、。{}・]+)・カードの枚数",
        "play",
        "count_subjects",
        "card_count",
        ("owner", "trait", "trait_right"),
    ),
    HistoryForm(
        "evolve.field_follower.exists",
        _TURN + _OWNER + r"の場のフォロワーが進化していたなら",
        "evolve",
        "exists",
        "truth",
    ),
    HistoryForm(
        "discard.hand.exists",
        _TURN + _OWNER + r"の手札を捨てていたなら",
        "discard",
        "exists",
        "truth",
    ),
    HistoryForm(
        "destroy.follower.exists",
        _TURN + _OWNER + r"のフォロワーが破壊されていたなら",
        "destroy",
        "exists",
        "truth",
    ),
    HistoryForm(
        "banish.ex_name.exists",
        _TURN + _OWNER + r"のEXエリアの『(?P<name>X)』が消滅していたなら",
        "banish",
        "exists",
        "truth",
        ("owner", "name"),
    ),
    HistoryForm(
        "field_to_graveyard.follower.exists",
        _TURN + _OWNER + r"のフォロワーが場から墓場に置かれていたなら",
        "field_to_graveyard",
        "exists",
        "truth",
    ),
    HistoryForm(
        "field_to_graveyard.follower.count_subjects",
        _TURN + r"場から墓場に置かれたフォロワーの数",
        "field_to_graveyard",
        "count_subjects",
        "card_count",
        (),
    ),
    HistoryForm(
        "return_to_hand.card.exists",
        _TURN + _OWNER + r"のカードが場から手札に戻っていたなら",
        "return_to_hand",
        "exists",
        "truth",
    ),
    HistoryForm(
        "return_to_hand.trait_follower.exclude_same_name.exists",
        _TURN
        + _OWNER
        + r"の「これと同名を除く"
        + _TRAIT
        + r"フォロワー」が場から手札に戻っていたなら",
        "return_to_hand",
        "exists",
        "truth",
        ("owner", "trait"),
    ),
    HistoryForm(
        "field_departure.name.count_subjects",
        _TURN + _OWNER + r"の場を離れた『(?P<name>X)』の数",
        "field_departure",
        "count_subjects",
        "card_count",
        ("owner", "name"),
    ),
    HistoryForm(
        "life_decrease.leader.count_facts",
        _TURN + _OWNER + r"のリーダーの(?:体力|\{体力\})が減少した回数",
        "life_decrease",
        "count_facts",
        "event_count",
    ),
    HistoryForm(
        "life_increase.leader.count_facts",
        _TURN + _OWNER + r"のリーダーの\{体力\}を\+した回数",
        "life_increase",
        "count_facts",
        "event_count",
    ),
    HistoryForm(
        "life_increase.leader.exists",
        _TURN + _OWNER + r"のリーダーの\{体力\}を\+していたなら",
        "life_increase",
        "exists",
        "truth",
    ),
    HistoryForm(
        "life_increase.leader.sum",
        _TURN + _OWNER + r"のリーダーの\{体力\}を\+" + _NUMBER + r"以上していたなら",
        "life_increase",
        "sum",
        "life",
    ),
    HistoryForm(
        "self_buff.attack_or_health.exists",
        _TURN + r"これの\{攻撃力\}や\{体力\}を\+していたなら",
        "self_buff_attack_or_health",
        "exists",
        "truth",
        (),
    ),
    HistoryForm(
        "self_buff.health.exists",
        _TURN + r"これの\{体力\}を\+していたなら",
        "self_buff_health",
        "exists",
        "truth",
        (),
    ),
    HistoryForm(
        "dice.exists",
        _TURN + _OWNER + r"がサイコロをふっていたなら",
        "dice",
        "exists",
        "truth",
    ),
    HistoryForm(
        "ub.exclude_execution.count_facts",
        r"これを含めず、"
        + _TURN
        + _OWNER
        + r"の\{UB\}能力が"
        + _NUMBER
        + r"回以上発動していたなら",
        "ub",
        "count_facts",
        "event_count",
        include_execution=False,
    ),
    HistoryForm(
        "earth_rite.stack.sum",
        _TURN
        + _OWNER
        + r"の【土の秘術】によって自分の場の【スタック】を"
        + _NUMBER
        + r"個以上取っていたなら",
        "earth_rite",
        "sum",
        "counter_count",
    ),
)
_AND = Descriptor(
    "history.play.both.exists.v1",
    "predicate.history.current_turn.v1",
    "history_condition",
    "Bool",
    "truth",
    tuple(
        Parameter(
            p, "PredicateExpr", "history_condition", "predicate.history.current_turn.v1"
        )
        for p in ("left", "right")
    ),
    (),
    "A2",
    ("X.predicate.And",),
)
_BOTH = re.compile(
    _TURN
    + _OWNER
    + r"が(?P<trait>[^『』「」、。{}・]+)・フォロワーと(?P<trait_right>[^『』「」、。{}・]+)・スペルをプレイしていたなら"
)
_BOTH_FORMS = tuple(
    HistoryForm(
        "play.trait_" + kind + ".exists",
        "",
        "play",
        "exists",
        "truth",
        ("owner", "trait"),
    )
    for kind in ("follower", "spell")
)
DESCRIPTORS = (*tuple(form.descriptor for form in (*_FORMS, *_BOTH_FORMS)), _AND)
_RULES = {d.id: d for d in DESCRIPTORS}


def _operand(
    raw: str,
    part: SourcePart,
    match: re.Match[str],
    name: str,
    classifier: Classifier,
    prefix: str,
) -> Leaf | None:
    span = Span(start=match.start(name), end=match.end(name))
    parameter = _PARAMETERS["trait" if name == "trait_right" else name]
    text = spelling(raw, part, span)
    value: TypedValue
    type_name: LeafType = parameter.type
    if name == "owner":
        value = "self" if text == "自分" else "opponent"
    else:
        category = "card_name" if name == "name" else "trait"
        terms = [
            t
            for t in classifier.terms.values()
            if t.category == category and t.source_ja == text
        ]
        if len(terms) != 1:
            return None
        value = (
            CardNameReference(kind="card_name", term_id=terms[0].id)
            if name == "name"
            else GlossaryReference(kind="glossary", key=terms[0].id)
        )
    item = leaf(type_name, parameter.role, parameter.domain, value, ((span,),), "N2.A2")
    return replace(
        item,
        slot=item.slot.model_copy(update={"name": prefix + "_" + name}),
        abstract=type_name != "CardName",
    )


def _nat(name: str, role: str, value: int, span: Span) -> Leaf:
    return Leaf(
        LeafSlot(
            name=name,
            type="Nat",
            role=role,
            domain=Domain(values=(), min=0, max=SAFE_INTEGER),
            required=True,
            occurrences=(span,),
        ),
        value,
        ((span,),),
        "N2.A2",
        abstract=False,
    )


def _expression(
    name: str, expr: ContextExpression | PredicateExpr, spans: tuple[Span, ...]
) -> Leaf:
    rule = _RULES.get(expr.expression_id) or DEFAULT_REGISTRY.definition(expr)
    if expr.expression_id == "condition.quantity.at_least.v1":
        spans = tuple(Span(start=s.end - 2, end=s.end) for s in spans)
    elif expr.expression_id == "quantity.scale.once.v1":
        spans = tuple(Span(start=s.end - 1, end=s.end) for s in spans)
    item = leaf(
        "QuantityExpr" if rule.result_type == "Nat" else "PredicateExpr",
        rule.role,
        rule.domain,
        expr,
        tuple((s,) for s in spans),
        "N2.A2",
    )
    return replace(
        item,
        slot=item.slot.model_copy(update={"name": name, "expression": expr}),
        abstract=False,
    )


def _consumer(text: str, start: int, end: int) -> str:
    if (
        "この能力は" in text[:start].rsplit("。", maxsplit=1)[-1]
        and "使える" in text[end:].split("。", maxsplit=1)[0]
    ):
        return "ability_availability"
    if "下記から" in text[:start] and text[end:].startswith("、代わりに"):
        return "mode_count"
    if "これをプレイする際" in text[:start] or "プレイでき" in text[end:]:
        return "play_availability"
    return "body_condition"


def _read_positions(text: str, match: re.Match[str]) -> tuple[Span, ...]:
    positions = [Span(start=match.start(), end=match.start() + 1)]
    if text[: match.start()].endswith("Xは「") and text[match.end() :].startswith(
        "」である"
    ):
        definition = match.start() - len("Xは「")
        # A later ability head or another definition makes the declaration ambiguous.
        uses = tuple(
            re.finditer(
                r"(?<![A-Za-z])X(?![A-Za-z])",
                re.sub(r"『[^』]*』", lambda m: " " * len(m[0]), text[:definition]),
            )
        )
        if (
            len(re.findall(r"Xは「", text)) == 1
            and uses
            and not re.search(
                ABILITY_HEAD + "|" + OPTION_LABEL, text[uses[0].end() : definition]
            )
        ):
            positions.extend(Span(start=m.start(), end=m.end()) for m in uses)
            positions.append(Span(start=definition, end=definition + 1))
    return tuple(sorted(positions, key=lambda s: s.start))


@dataclass
class _HistorySources:
    raw: str
    part: SourcePart
    classifier: Classifier
    source: OccurrenceKey
    additions: list[Leaf] = field(default_factory=list)
    ports: list[Port] = field(default_factory=list)
    scopes: list[str] = field(default_factory=list)
    issues: set[str] = field(default_factory=set)

    def player(self, match: re.Match[str], name: str, scope: str, role: str) -> str:
        port_name = name + "_" + role
        self.scopes.append(scope)
        self.ports.append(
            context_port(
                port_name,
                "history_" + role,
                scope,
                PlayerRef(
                    kind="PlayerRef",
                    player="opponent"
                    if match.groupdict().get("owner") == "相手"
                    else "self",
                ),
                "current_turn",
                self.source,
            )
        )
        return port_name

    def query(self, match: re.Match[str], form: HistoryForm, name: str) -> None:
        operands = [
            _operand(self.raw, self.part, match, p, self.classifier, name)
            for p in form.operands
        ]
        if any(o is None for o in operands):
            self.issues.add("missing_history_operand")
            return
        selected = [o for o in operands if o is not None]
        text = self.part.canonical_source
        scope = source_scope(
            text,
            match.start(),
            match.end(),
            _consumer(text, match.start(), match.end()),
        )
        imports = self.query_imports(match, form, name, scope)
        expr: ContextExpression | PredicateExpr
        if form.aggregate == "exists":
            expr = PredicateExpr(
                kind="predicate",
                expression_id=form.descriptor.id,
                leaves=tuple(o.slot.name for o in selected),
                imports=imports,
                scope=scope,
            )
        else:
            expr = ContextExpression(
                kind="context_expression",
                expression_id=form.descriptor.id,
                leaves=tuple(o.slot.name for o in selected),
                imports=imports,
                scope=scope,
            )
        self.additions.extend(
            (*selected, _expression(name, expr, _read_positions(text, match)))
        )
        self.condition(match, name, scope)
        self.scale(match, name, scope)

    def query_imports(
        self, match: re.Match[str], form: HistoryForm, name: str, scope: str
    ) -> dict[str, str]:
        imports = {}
        self.scopes.append(scope)
        if any(p.name == form.player_role for p in form.import_signature):
            imports[form.player_role] = self.player(
                match, name, scope, form.player_role
            )
        if any(p.name == "subject" for p in form.import_signature):
            port_name = name + "_subject"
            subject = ObjectGeneration(
                object_id="source-holder:"
                + hash_payload(
                    {"source": self.source.model_dump(mode="json"), "scope": scope}
                ),
                generation=0,
            )
            self.ports.append(
                context_port(
                    port_name,
                    "history_subject",
                    scope,
                    ObjectSetRef(kind="ObjectSet", objects=(subject,), origin="holder"),
                    "current_turn",
                    self.source,
                )
            )
            imports["subject"] = port_name
        return imports

    def condition(self, match: re.Match[str], name: str, scope: str) -> None:
        suffix = re.match(
            r"」?が(?P<threshold>N)(?:枚|回)以上なら",
            self.part.canonical_source[match.end() :],
        )
        if match.groupdict().get("threshold"):
            span = Span(start=match.start("threshold"), end=match.end("threshold"))
        elif suffix:
            span = Span(
                start=match.end() + suffix.start("threshold"),
                end=match.end() + suffix.end("threshold"),
            )
        else:
            return
        operand = _nat(
            name + "_threshold",
            "comparison_threshold",
            int(spelling(self.raw, self.part, span)),
            span,
        )
        expr = PredicateExpr(
            kind="predicate",
            expression_id="condition.quantity.at_least.v1",
            leaves=(name, operand.slot.name),
            imports={},
            scope=scope,
        )
        self.additions.extend(
            (
                operand,
                _expression(
                    name + "_condition",
                    expr,
                    (
                        Span(
                            start=match.start(),
                            end=match.end() + (suffix.end() if suffix else 0),
                        ),
                    ),
                ),
            )
        )

    def scale(self, match: re.Match[str], name: str, scope: str) -> None:
        scale = re.match(
            r"」の(?P<factor>N)倍", self.part.canonical_source[match.end() :]
        )
        if not scale:
            return
        span = Span(
            start=match.end() + scale.start("factor"),
            end=match.end() + scale.end("factor"),
        )
        factor = _nat(
            name + "_factor",
            "constant_multiplier",
            int(spelling(self.raw, self.part, span)),
            span,
        )
        expr = ContextExpression(
            kind="context_expression",
            expression_id="quantity.scale.once.v1",
            leaves=(name, factor.slot.name),
            imports={},
            scope=scope,
        )
        self.additions.extend(
            (
                factor,
                _expression(
                    name + "_scaled",
                    expr,
                    (Span(start=match.start(), end=match.end() + scale.end()),),
                ),
            )
        )

    def both(self, match: re.Match[str], prefix: str) -> None:
        operands = [
            _operand(self.raw, self.part, match, p, self.classifier, prefix)
            for p in ("owner", "trait", "trait_right")
        ]
        if any(o is None for o in operands):
            self.issues.add("missing_history_operand")
            return
        selected = [o for o in operands if o is not None]
        self.additions.extend(selected)
        scope = source_scope(
            self.part.canonical_source, match.start(), match.end(), "body_condition"
        )
        actor = self.player(match, prefix, scope, "actor")
        for side, form in enumerate(_BOTH_FORMS):
            expr = PredicateExpr(
                kind="predicate",
                expression_id=form.descriptor.id,
                leaves=(selected[0].slot.name, selected[side + 1].slot.name),
                imports={"actor": actor},
                scope=scope,
            )
            self.additions.append(
                _expression(
                    prefix + str(side),
                    expr,
                    (
                        Span(
                            start=match.start()
                            if side == 0
                            else match.end() - len("スペルをプレイしていたなら"),
                            end=match.start() + 1
                            if side == 0
                            else match.end() - len("スペルをプレイしていたなら") + 1,
                        ),
                    ),
                )
            )
        expr = PredicateExpr(
            kind="predicate",
            expression_id=_AND.id,
            leaves=(prefix + "0", prefix + "1"),
            imports={},
            scope=scope,
        )
        self.additions.append(
            _expression(prefix, expr, (Span(start=match.end() - 2, end=match.end()),))
        )

    def scan(self) -> None:
        text = self.part.canonical_source
        matches = sorted(
            ((m, form) for form in _FORMS for m in re.finditer(form.pattern, text)),
            key=lambda pair: (pair[0].start(), -pair[0].end()),
        )
        covered: list[Span] = []
        for number, (match, form) in enumerate(matches):
            if any(s.start <= match.start() < s.end for s in covered):
                continue
            if text[: match.start()].endswith("これを含めず、"):
                continue
            self.query(match, form, f"p2_history_{number}")
            covered.append(Span(start=match.start(), end=match.end()))
        for number, match in enumerate(_BOTH.finditer(text)):
            self.both(match, f"p2_both_{number}")


def prepare_history(
    raw: str,
    prepared: DerivedField,
    classifier: Classifier,
    context: CardContext | None,
) -> DerivedField:
    """Register source structure without claiming a runtime history evaluator exists."""
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
        if original.source_span.role not in {"body", "reminder"}:
            continue
        found = _HistorySources(
            raw, original, classifier, occurrence(derived.field.source, original)
        )
        found.scan()
        if found.additions:
            parts[index], recognized[index], rows[index], _ = update_source(
                raw,
                original,
                parts[index],
                recognized[index],
                found.additions,
                classifier,
            )
            interfaces[index] = interface(
                interfaces[index], tuple(found.scopes), tuple(found.ports)
            )
        if found.issues:
            recognized[index] = replace(
                recognized[index],
                issues=tuple(sorted({*recognized[index].issues, *found.issues})),
                semantics=None,
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


HISTORY_FAMILY = SourceFamily("current_turn_history", DESCRIPTORS, prepare_history)


def name_filter_spans(text: str) -> tuple[tuple[int, int], ...]:
    """The historical query owns filters on past event subjects, including their names."""
    return tuple(
        (match.start(), match.end())
        for form in _FORMS
        if "exclude_same_name" in form.key
        for match in re.finditer(form.pattern, text)
    )
