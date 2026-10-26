"""Source-local construction of A1/A4 reads, without evaluating card effects."""

import re
from dataclasses import dataclass, field, replace
from typing import TYPE_CHECKING, Literal

from sve_carddb.contracts.four_layer import (
    CapturedValueRef,
    CardNameReference,
    ContextExpression,
    Domain,
    Expression,
    GlossaryReference,
    LeafSlot,
    ObjectGeneration,
    ObjectSetRef,
    OccurrenceKey,
    PlayerRef,
    Port,
    PortEvidence,
    ReceiptRef,
    Span,
    VocabularyReference,
    hash_payload,
)
from sve_carddb.contracts.n0 import SAFE_INTEGER
from sve_carddb.domains.translations.four_layer_ability import HEAD, OPTION_LABEL
from sve_carddb.domains.translations.four_layer_event_preparation import source_identity
from sve_carddb.domains.translations.four_layer_expressions import (
    DEFINITIONS,
    DOMAIN_CODE,
)
from sve_carddb.domains.translations.four_layer_n1_leaves import Leaf, leaf
from sve_carddb.domains.translations.four_layer_n2_read_descriptors import (
    PARAMETERS,
    REGISTRY,
)
from sve_carddb.domains.translations.four_layer_n3_shapes import CHOSEN_X_EXPRESSION
from sve_carddb.domains.translations.four_layer_normalizer import normalize_source
from sve_carddb.domains.translations.recognition.provenance import merged

if TYPE_CHECKING:
    from sve_carddb.contracts.four_layer import CaptureTime, PortValue
    from sve_carddb.contracts.source_binding import SourceDescriptor, TypedValue
    from sve_carddb.domains.translations.four_layer_classification import (
        Classifier,
        Recognized,
    )
    from sve_carddb.domains.translations.four_layer_normalizer import SourcePart

_ZONES = {
    "場": "battlefield",
    "手札": "hand",
    "デッキ": "deck",
    "墓場": "graveyard",
    "EXエリア": "ex",
    "消滅領域": "banish",
    "エボルヴデッキ": "evolve_deck",
}
_PAIR_SIZE = 2
_KINDS = {"フォロワー": "follower", "アミュレット": "amulet", "スペル": "spell"}

_MODE = re.compile(OPTION_LABEL)
_X = re.compile(r"(?<![A-Za-z0-9_])X(?![A-Za-z0-9_])")
_QUOTE = re.compile(
    r"「(?P<operand>(?:[^「」]|「[^「」]*」)+)」(?:(?P<same>と同じ)|の(?P<factor>N)倍)"
)
_DEFINE = re.compile(r"Xは「(?P<operand>[^「」]+)」である。")


def origins(part: SourcePart, span: Span) -> tuple[Span, ...]:
    """All matching and provider identities retain exact original source coordinates."""
    return merged(
        tuple(s for u in part.units[span.start : span.end] for s in u.origins)
    )


def project(original: SourcePart, current: SourcePart, span: Span) -> tuple[Span, ...]:
    """N1 markers may cover multiple original units; project through their provenance."""
    raw = origins(original, span)
    return merged(
        tuple(
            Span(start=i, end=i + 1)
            for i, u in enumerate(current.units)
            if any(a.start < b.end and b.start < a.end for a in u.origins for b in raw)
        )
    )


@dataclass(frozen=True)
class Region:
    start: int
    end: int
    id: str
    parent: str | None
    kind: str


def regions(text: str) -> tuple[Region, ...]:
    """Granted abilities and choice siblings never share an X or object provider."""
    grants: list[tuple[int, int]] = []
    stack: list[int] = []
    for i, char in enumerate(text):
        if char == "「":
            stack.append(i)
        elif char == "」" and stack:
            start = stack.pop()
            if text[i + 1 :].startswith(("を持つ", "を与える")) and re.search(
                r"【|とき、|ターンごとに", text[start:i]
            ):
                grants.append((start + 1, i))
    heads = [
        m.start()
        for m in HEAD.finditer(text)
        if not any(a <= m.start() < b for a, b in grants)
    ]
    starts = sorted({0, *heads})
    result = [
        Region(
            start,
            starts[i + 1] if i + 1 < len(starts) else len(text),
            f"read_ability_{i}",
            None,
            "ability",
        )
        for i, start in enumerate(starts)
    ]
    for start, end in grants:
        parent_id = next(r.id for r in result if r.start <= start < r.end)
        result.append(Region(start, end, f"read_grant_{start}", parent_id, "ability"))
    for parent in tuple(result):
        modes = [
            m.start()
            for m in _MODE.finditer(text, parent.start, parent.end)
            if not any(
                r.id != parent.id
                and r.start <= m.start() < r.end
                and r.parent == parent.id
                for r in result
            )
        ]
        for i, start in enumerate(modes):
            result.append(
                Region(
                    start,
                    modes[i + 1] if i + 1 < len(modes) else parent.end,
                    f"{parent.id}_mode_{i}",
                    parent.id,
                    "branch",
                )
            )
    return tuple(result)


@dataclass
class ReadSources:
    raw: str
    source: SourceDescriptor
    original: SourcePart
    current: SourcePart
    found: Recognized
    classifier: Classifier
    leaves: list[Leaf] = field(default_factory=list)
    ports: list[Port] = field(default_factory=list)
    pending: set[str] = field(default_factory=set)

    @property
    def _text(self) -> str:
        return self.original.canonical_source

    def _region(self, position: int) -> Region:
        """The smallest lexical region is the consuming ability or branch."""
        return min(
            (r for r in regions(self._text) if r.start <= position < r.end),
            key=lambda r: r.end - r.start,
        )

    def _scope(self, position: int) -> str:
        return self._region(position).id

    def _spelling(self, span: Span) -> str:
        """Raw spelling supplies values after Unicode normalization erased their digits."""
        return "".join(self.raw[s.start : s.end] for s in origins(self.original, span))

    def _scalar(self, parameter: str, value: TypedValue, span: Span) -> str:
        """Formals use distinct positions even when their resolved values are equal."""
        p = PARAMETERS[parameter]
        name = f"read_arg_{len(self.leaves)}"
        positions = project(self.original, self.current, span)
        item = (
            leaf(p.type, p.role, p.domain, value, (positions,), "N2-A1")
            if p.domain
            else Leaf(
                LeafSlot(
                    name=name,
                    type="Nat",
                    role=p.role,
                    domain=Domain(values=(), min=0, max=SAFE_INTEGER),
                    required=True,
                    occurrences=positions,
                ),
                value,
                (positions,),
                "N2-A1",
                abstract=False,
            )
        )
        self.leaves.append(
            replace(
                item, slot=item.slot.model_copy(update={"name": name}), abstract=False
            )
        )
        return name

    def _expression(
        self,
        identifier: str,
        arguments: tuple[str, ...],
        positions: tuple[Span, ...],
        imports: dict[str, str] | None = None,
    ) -> str:
        """A descriptor and its source scope enter the leaf schema and frame identity."""
        rule = REGISTRY.descriptors.get(identifier)
        name = f"read_expr_{len(self.leaves)}"
        expr: Expression | ContextExpression = (
            Expression(kind="expression", expression_id=identifier, leaves=arguments)
            if identifier in DEFINITIONS
            else ContextExpression(
                kind="context_expression",
                expression_id=identifier,
                leaves=arguments,
                imports=imports or {},
                scope=self._scope(positions[0].start),
            )
        )
        occupied = {
            i
            for item in self.leaves
            for group in item.positions
            for s in group
            for i in range(s.start, s.end)
        }
        positions = tuple(sorted(positions, key=lambda s: s.start))
        projected = tuple(
            merged(
                tuple(
                    Span(start=i, end=i + 1)
                    for s in project(self.original, self.current, span)
                    for i in range(s.start, s.end)
                    if i not in occupied
                )
            )
            for span in positions
        )
        if not all(projected):
            raise ValueError("Read structure has no independent source anchor")
        item = leaf(
            "QuantityExpr",
            rule.role if rule is not None else "count_expression",
            rule.domain if rule is not None else DOMAIN_CODE,
            expr,
            projected,
            "N2-A4" if "chosen_x" in identifier else "N2-A1",
        )
        self.leaves.append(
            replace(
                item,
                slot=item.slot.model_copy(
                    update={
                        "name": name,
                        "expression": expr
                        if isinstance(expr, ContextExpression)
                        else None,
                    }
                ),
                abstract=False,
            )
        )
        return name

    def _port(
        self,
        role: str,
        value: PortValue,
        capture: CaptureTime,
        provider: Span,
        consumer: int,
        source_leaves: tuple[str, ...] = (),
    ) -> str:
        """These are symbolic source bindings, never runtime object IDs or evaluated numbers."""
        name = f"read_port_{len(self.ports)}"
        self.ports.append(
            Port(
                name=name,
                type=value.kind,
                source_role=role,
                scope=self._scope(consumer),
                evidence=PortEvidence(
                    source=OccurrenceKey(
                        owner=self.source.owner,
                        field=self.source.field,
                        ordinal=self.source.ordinal,
                        source_hash=self.source.source_hash,
                        line_ordinal=self.original.line_ordinal,
                        role=self.original.source_span.role,
                        segments=origins(self.original, provider),
                    ),
                    capture=capture,
                    value=value,
                    source_leaves=source_leaves,
                ),
            )
        )
        return name

    def _identity(self, provider: Span, kind: str) -> str:
        return (
            "source-"
            + kind
            + ":"
            + hash_payload(
                {
                    "source": source_identity(self.source),
                    "scope": self._scope(provider.start),
                    "segments": []
                    if kind == "holder"
                    else [
                        s.model_dump(mode="json")
                        for s in origins(self.original, provider)
                    ],
                }
            )
        )

    def _reference(self, type_: str, span: Span) -> TypedValue | None:
        """Only already adopted source references can become required operands."""
        target = origins(self.original, span)
        values = [
            self.found.values[s.name]
            for s in self.found.schema.slots
            if s.type == type_
            and any(
                o.slot == s.name and o.raw_spans == target
                for o in self.found.occurrences
            )
        ]
        return values[0] if len(values) == 1 else None

    def _term(self, category: str, span: Span) -> GlossaryReference | None:
        spelling = self._spelling(span)
        terms = [
            t
            for t in self.classifier.terms.values()
            if t.category == category and t.source_ja == spelling
        ]
        return (
            GlossaryReference(kind="glossary", key=terms[0].id)
            if len(terms) == 1
            else None
        )

    def _filters(self, text: str, start: int) -> dict[str, str] | None:
        """One registered modifier plus one object kind is the sealed source grammar."""
        if not text:
            return {}
        if text == "『X』":
            span = Span(start=start + 1, end=start + 2)
            value = self._reference("CardName", span)
            if not isinstance(value, CardNameReference):
                self.pending.add("a1_missing_card_name_provider")
                return None
            return {"name": self._scalar("name", value, span)}
        kind = next(
            (k for k in (*_KINDS, "カード", "トークン") if text.endswith(k)), None
        )
        if kind is None:
            return None
        end = start + len(text)
        span = Span(start=end - len(kind), end=end)
        result = {}
        if kind in _KINDS:
            result["kind"] = self._scalar(
                "kind",
                VocabularyReference(kind="vocabulary", key=("type", _KINDS[kind])),
                span,
            )
        elif kind == "カード":
            value = self._term("rule_term", span)
            if value is None:
                self.pending.add("a1_missing_card_concept")
                return None
            result["card"] = self._scalar("card", value, span)
        else:
            result["token_status"] = self._scalar("token_status", "token", span)
        modifier = self._modifier(text[: -len(kind)], start)
        if modifier is not None:
            result.update(modifier)
        return result if modifier is not None else None

    def _modifier(self, modifier: str, start: int) -> dict[str, str] | None:
        result: dict[str, str] = {}
        if modifier:
            category = (
                "keyword"
                if modifier.startswith("【") and modifier.endswith("】を持つ")
                else "class"
                if modifier.startswith("{") and modifier.endswith("}")
                else "trait"
            )
            offset = 1 if category in {"keyword", "class"} else 0
            stop = len(modifier) - (4 if category == "keyword" else 1)
            if category == "trait" and not modifier.endswith("・"):
                return None
            span = Span(start=start + offset, end=start + stop)
            if category == "class":
                resolution = self.classifier.references.proposed_vocabulary(
                    "class", self._spelling(span)
                )
                code = (
                    resolution.target.get("vocabulary_code")
                    if resolution.target
                    else None
                )
                value: GlossaryReference | VocabularyReference | None = (
                    VocabularyReference(kind="vocabulary", key=("class", code))
                    if isinstance(code, str) and not resolution.issues
                    else None
                )
            else:
                value = self._term(category, span)
            if value is None:
                self.pending.add("a1_missing_" + category + "_provider")
                return None
            result[category] = self._scalar(category, value, span)
        return result

    def _zone(self, text: str, start: int, positions: tuple[Span, ...]) -> str | None:
        match = re.fullmatch(
            r"(?P<owner>自分|相手)の(?P<zone>場|手札|デッキ|墓場|EXエリア|消滅領域|エボルヴデッキ)の(?P<exclude>他の)?(?P<filter>.*?)の?(?:枚数|数)",
            text,
        )
        if match is None:
            return None
        args = {
            "owner": self._scalar(
                "owner",
                "self" if match["owner"] == "自分" else "opponent",
                Span(
                    start=start + match.start("owner"), end=start + match.end("owner")
                ),
            ),
            "zone": self._scalar(
                "zone",
                (_ZONES[match["zone"]],),
                Span(start=start + match.start("zone"), end=start + match.end("zone")),
            ),
        }
        faceup = match["zone"] == "エボルヴデッキ"
        noun = match["filter"].removesuffix("の")
        if faceup:
            return self._faceup(match, start, args, positions)
        filters = self._filters(noun, start + match.start("filter"))
        if filters is None:
            return None
        args.update(filters)
        params = tuple(
            p
            for p in (
                "owner",
                "zone",
                "class",
                "keyword",
                "token_status",
                "kind",
                "card",
                "trait",
                "name",
            )
            if p in args
        )
        qualifiers = (("exclude_this",) if match["exclude"] else ()) + (
            ("token",) if "token_status" in args else ()
        )
        identifier = ".".join(("count.current_zone", *params, *qualifiers, "v1"))
        return (
            self._expression(identifier, tuple(args[p] for p in params), positions)
            if identifier in DEFINITIONS
            else None
        )

    def _faceup(
        self,
        match: re.Match[str],
        start: int,
        args: dict[str, str],
        positions: tuple[Span, ...],
    ) -> str | None:
        noun = match["filter"].removesuffix("の")
        if not noun.startswith("表向きの") or match["exclude"]:
            return None
        noun = noun.removeprefix("表向きの")
        location = start + match.start("filter") + len("表向きの")
        if noun == "『X』":
            filters = self._filters(noun, location)
            if filters is None:
                return None
            return self._expression(
                "read.faceup_evolve.count.v1",
                (args["owner"], args["zone"], filters["name"]),
                positions,
            )
        if noun == "エボルヴフォロワー":
            return self._expression(
                "read.faceup_evolve.all.v1", tuple(args.values()), positions
            )
        if noun.endswith("・エボルヴフォロワー"):
            span = Span(
                start=location,
                end=location + len(noun) - len("・エボルヴフォロワー"),
            )
            value = self._term("trait", span)
            if value is not None:
                args["trait"] = self._scalar("trait", value, span)
                return self._expression(
                    "read.faceup_evolve.trait.v1", tuple(args.values()), positions
                )
        return None

    def _provider(
        self, pattern: str, consumer: int, *, following: bool = False
    ) -> Span | None:
        region = self._region(consumer)
        matches = [
            m
            for m in re.compile(pattern).finditer(self._text, region.start, region.end)
            if self._scope(m.start()) == region.id
            and self._scope(m.end() - 1) == region.id
            and (following or m.end() <= consumer)
        ]
        if len(matches) != 1:
            self.pending.add(
                "a1_ambiguous_object_provider"
                if matches
                else "a1_missing_object_provider"
            )
            return None
        return Span(start=matches[0].start(), end=matches[0].end())

    def _object(self, text: str, start: int, positions: tuple[Span, ...]) -> str | None:
        match = re.fullmatch(
            r"(?P<subject>これ|それ|選んだフォロワー|前者|後者|公開したカード|捨てたカード|捨てたフォロワー|墓場に置いたフォロワー|出たアミュレット|EXエリアに置いたカード|これによって消滅した(?:\{[^{}]+\}フォロワー|「\{[^{}]+\}でないフォロワー」)|これによって場に出した[^・]+・フォロワー)の(?P<property>攻撃力|体力|元のコスト|コスト)",
            text,
        )
        if match is None:
            return None
        prop = {
            "攻撃力": "attack",
            "体力": "health",
            "元のコスト": "original_cost",
            "コスト": "effective_cost",
        }[match["property"]]
        subject = match["subject"]
        consumer = positions[0].start
        capture: CaptureTime = "live" if subject == "これ" else "selection"
        origin: Literal["holder", "selection", "event_subject"] = (
            "holder" if subject == "これ" else "selection"
        )
        provider, capture = self._object_provider(subject, start, consumer)
        if provider is None:
            return None
        quantities = tuple(
            re.finditer(r"N(?:枚|体|つ)", self._text[provider.start : provider.end])
        )
        if quantities:
            number = quantities[-1]
            span = Span(
                start=provider.start + number.start(),
                end=provider.start + number.start() + 1,
            )
            if int(self._spelling(span)) != 1:
                self.pending.add("a1_non_single_object_provider")
                return None
        obj = ObjectSetRef(
            kind="ObjectSet",
            objects=(
                ObjectGeneration(
                    object_id=self._identity(
                        provider, "holder" if subject == "これ" else "object"
                    ),
                    generation=0,
                ),
            ),
            origin=origin,
        )
        if subject == "出たアミュレット":
            capture = "at_event"
            obj = obj.model_copy(update={"origin": "event_subject"})
        args, qualifier = self._object_qualifier(subject, start)
        if args is None or not self._valid_property(subject, qualifier, prop):
            return None
        port = self._port("read_subject", obj, capture, provider, consumer)
        identifier = (
            "read.object." + prop + ".v1"
            if subject == "これ" and prop != "health"
            else "read.holder.health.v1"
            if subject == "これ"
            else "read."
            + ("lki" if capture == "lki" else "selected")
            + "."
            + prop
            + ".v1"
        )
        if subject == "出たアミュレット":
            identifier = "read.event_subject.effective_cost.v1"
        if qualifier:
            identifier = "read.result_object." + qualifier + ".v1"
        return self._expression(identifier, args, positions, {"subject": port})

    @staticmethod
    def _valid_property(subject: str, qualifier: str | None, property_: str) -> bool:
        if subject == "出たアミュレット":
            return property_ == "effective_cost"
        if qualifier and qualifier.startswith("banished"):
            return property_ == "attack"
        return qualifier != "entered_trait_health" or property_ == "health"

    def _object_qualifier(
        self, subject: str, start: int
    ) -> tuple[tuple[str, ...] | None, str | None]:
        if subject.startswith("これによって消滅した"):
            noun = subject.removeprefix("これによって消滅した")
            negative = noun.startswith("「")
            noun = noun.removeprefix("「").removesuffix("」")
            noun = noun.replace("でない", "")
            class_match = re.search(r"\{(?P<class>[^{}]+)\}", subject)
            if class_match is None:
                return None, None
            resolution = self.classifier.references.proposed_vocabulary(
                "class",
                self._spelling(
                    Span(
                        start=start + class_match.start("class"),
                        end=start + class_match.end("class"),
                    )
                ),
            )
            code = (
                resolution.target.get("vocabulary_code") if resolution.target else None
            )
            if not isinstance(code, str) or resolution.issues:
                self.pending.add("a1_missing_class_provider")
                return None, None
            arg = self._scalar(
                "class",
                VocabularyReference(kind="vocabulary", key=("class", code)),
                Span(
                    start=start + class_match.start("class"),
                    end=start + class_match.end("class"),
                ),
            )
            kind_start = start + subject.index("フォロワー")
            kind = self._scalar(
                "kind",
                VocabularyReference(kind="vocabulary", key=("type", "follower")),
                Span(start=kind_start, end=kind_start + len("フォロワー")),
            )
            return (
                arg,
                kind,
            ), "banished_not_class_attack" if negative else "banished_class_attack"
        if subject.startswith("これによって場に出した"):
            prefix = "これによって場に出した"
            args = self._filters(subject.removeprefix(prefix), start + len(prefix))
            if args and set(args) == {"kind", "trait"}:
                return (args["kind"], args["trait"]), "entered_trait_health"
            return None, None
        return (), None

    def _object_provider(
        self, subject: str, start: int, consumer: int
    ) -> tuple[Span | None, CaptureTime]:
        capture: CaptureTime = "live" if subject == "これ" else "selection"
        provider: Span | None
        if subject == "これ":
            provider = Span(start=start, end=start + len("これ"))
        else:
            pattern = r"[^。:]+(?:選ぶ|選び、)"
            if subject in {"前者", "後者"}:
                provider = self._ordered_provider(subject, consumer)
            else:
                pattern, capture = self._object_operation(subject, pattern, capture)
                provider = self._provider(pattern, consumer)
            if provider is None:
                return None, capture
            # Returning or moving the object is a zone change just like destruction.
            if re.search(
                r"それを(?:破壊|消滅|墓場|手札に戻|デッキ|EXエリアに置)",
                self._text[provider.end : consumer],
            ):
                capture = "lki"
        return provider, capture

    @staticmethod
    def _object_operation(
        subject: str, default: str, capture: CaptureTime
    ) -> tuple[str, CaptureTime]:
        patterns: tuple[tuple[str, str, CaptureTime], ...] = (
            ("公開した", r"[^。:]*?公開(?:する|し[、。])", "selection"),
            ("捨てた", r"[^。:]*?捨て(?:る|[、。])", "lki"),
            ("墓場に置いた", r"[^。:]*?墓場に置(?:く|き[、。])", "lki"),
            ("出た", r"[^。:]*?場にアミュレットが出たとき、", "selection"),
            (
                "これによって場に出した",
                r"[^。:]*?場に出(?:す|し[、。]|してよい。)",
                "selection",
            ),
            ("EXエリアに置いた", r"[^。:]*?EXエリアに置(?:く|き[、。])", "selection"),
            (
                "これによって消滅した",
                r"[^。:]*?消滅(?::|させる|させ[、。]|する)",
                "lki",
            ),
        )
        return next(
            (
                (pattern, time)
                for prefix, pattern, time in patterns
                if subject.startswith(prefix)
            ),
            (default, capture),
        )

    def _ordered_provider(self, subject: str, consumer: int) -> Span | None:
        region = self._region(consumer)
        pattern = r"(?P<first>[^。:]*?)(?<!か)(?P<link>と)(?P<last>[^。:]*?)を選ぶ。"
        matches = [
            m
            for m in re.compile(pattern).finditer(self._text, region.start, consumer)
            if self._scope(m.start()) == region.id
            and self._scope(m.end() - 1) == region.id
        ]
        if len(matches) != 1:
            self.pending.add("a1_missing_ordered_selection_provider")
            return None
        match = matches[0]
        group = "first" if subject == "前者" else "last"
        return Span(start=match.start(group), end=match.end(group))

    def read(self, operand: str, start: int, positions: tuple[Span, ...]) -> str | None:
        """Recognize only sealed reads and the three finite arithmetic surfaces."""
        checkpoint = len(self.leaves), len(self.ports)
        result = self._read(operand, start, positions)
        if result is None:
            del self.leaves[checkpoint[0] :]
            del self.ports[checkpoint[1] :]
        return result

    def _read(
        self, operand: str, start: int, positions: tuple[Span, ...]
    ) -> str | None:
        if operand.endswith("の半分") and operand.removesuffix("の半分") in {
            "これの攻撃力",
            "捨てたフォロワーのコスト",
        }:
            span = Span(start=start, end=start + len(operand) - len("の半分"))
            value = self.read(operand.removesuffix("の半分"), start, (span,))
            return (
                self._expression("read.half_up.v1", (value,), positions)
                if value
                else None
            )
        offset = re.fullmatch(r"(?P<base>.+?)(?P<operator>[+-])(?P<offset>N)", operand)
        if offset:
            span = Span(
                start=start + offset.start("offset"), end=start + offset.end("offset")
            )
            if (offset["base"], offset["operator"], int(self._spelling(span))) not in {
                ("それの体力", "-", 1),
                ("選んだフォロワーの元のコスト", "+", 2),
                ("これによって捨てた手札の枚数", "+", 1),
            }:
                return None
            value = self.read(
                offset["base"],
                start,
                (Span(start=start, end=start + len(offset["base"])),),
            )
            if value:
                amount = self._scalar("offset", int(self._spelling(span)), span)
                return self._expression(
                    "read.offset."
                    + ("add" if offset["operator"] == "+" else "subtract")
                    + ".v1",
                    (value, amount),
                    positions,
                )
            return None
        if "このターン" in operand or "カード名の種類数" in operand:
            return (
                None
                if "このターン" in operand
                else self._name_count(start, operand, positions)
            )
        return (
            self._immediate(operand, start, positions)
            or self._counter(operand, start, positions)
            or self._special_zone(operand, start, positions)
            or self._zone(operand, start, positions)
            or self._object(operand, start, positions)
            or self._receipt(operand, start, positions)
        )

    def _name_count(
        self, start: int, operand: str, positions: tuple[Span, ...]
    ) -> str | None:
        covered = origins(self.original, Span(start=start, end=start + len(operand)))
        providers = [
            slot
            for slot in self.found.schema.slots
            if isinstance(value := self.found.values[slot.name], ContextExpression)
            and value.expression_id == "name.distinct_count.objects.v1"
            and any(
                o.slot == slot.name
                and o.raw_spans
                and all(
                    any(a.start <= b.start < b.end <= a.end for a in covered)
                    for b in o.raw_spans
                )
                for o in self.found.occurrences
            )
        ]
        if len(providers) != 1:
            self.pending.add("a1_pending_a3_distinct_name_provider")
            return None
        slot = providers[0]
        uses = tuple(o for o in self.found.occurrences if o.slot == slot.name)
        groups = tuple(o.canonical_spans for o in uses)
        extra = tuple(
            project(self.original, self.current, p)
            for p in positions
            if p.end <= start or p.start >= start + len(operand)
        )
        groups = tuple(
            sorted(dict.fromkeys((*groups, *extra)), key=lambda g: g[0].start)
        )
        self.leaves.append(
            Leaf(
                slot.model_copy(
                    update={"occurrences": merged(tuple(s for g in groups for s in g))}
                ),
                self.found.values[slot.name],
                groups,
                "N2-A1",
                abstract=False,
            )
        )
        return slot.name

    def _special_zone(
        self, operand: str, start: int, positions: tuple[Span, ...]
    ) -> str | None:
        distinct = re.fullmatch(
            r"自分の(?P<zone>墓場|EXエリア)にある(?P<card>カード)の元のコストの種類数",
            operand,
        )
        placed = re.fullmatch(
            r"自分の(?P<zone>エボルヴデッキ)に表向きで置いたエボルヴフォロワーの枚数",
            operand,
        )
        match = distinct or placed
        if match:
            owner = self._scalar(
                "owner", "self", Span(start=start, end=start + len("自分"))
            )
            zone = self._scalar(
                "zone",
                (_ZONES[match["zone"]],),
                Span(start=start + match.start("zone"), end=start + match.end("zone")),
            )
            args: tuple[str, ...] = (owner, zone)
            if distinct:
                span = Span(
                    start=start + distinct.start("card"),
                    end=start + distinct.end("card"),
                )
                value = self._term("rule_term", span)
                if value is None:
                    self.pending.add("a1_missing_card_concept")
                    return None
                args = (*args, self._scalar("card", value, span))
            return self._expression(
                "read.distinct_original_cost.zone.v1"
                if distinct
                else "read.faceup_evolve.placed.v1",
                args,
                positions,
            )
        if operand == "それのプレイヤーの手札の枚数":
            provider = self._provider(r"[^。:]+(?:選ぶ|選び、)", positions[0].start)
            if provider:
                zone = self._scalar(
                    "zone",
                    ("hand",),
                    Span(
                        start=start + len("それのプレイヤーの"),
                        end=start + len("それのプレイヤーの手札"),
                    ),
                )
                port = self._port(
                    "read_subject",
                    ObjectSetRef(
                        kind="ObjectSet",
                        objects=(
                            ObjectGeneration(
                                object_id=self._identity(provider, "object"),
                                generation=0,
                            ),
                        ),
                        origin="selection",
                    ),
                    "selection",
                    provider,
                    positions[0].start,
                )
                return self._expression(
                    "read.subject_player.hand_count.v1",
                    (zone,),
                    positions,
                    {"subject": port},
                )
        return None

    def _immediate(
        self, operand: str, start: int, positions: tuple[Span, ...]
    ) -> str | None:
        if operand == "自分のPP最大値":
            span = Span(start=start, end=start + len("自分"))
            owner = self._scalar("owner", "self", span)
            port = self._port(
                "read_player",
                PlayerRef(kind="PlayerRef", player="self"),
                "live",
                span,
                positions[0].start,
                source_leaves=(owner,),
            )
            return self._expression(
                "read.player.pp_maximum.v1", (), positions, {"player": port}
            )
        if operand == "出た目":
            provider = self._provider(
                r"サイコロをふる。|自分がサイコロをふったとき、", positions[0].start
            )
            if provider:
                port = self._port(
                    "dice_result",
                    CapturedValueRef(
                        kind="CapturedValue",
                        binding_id=self._identity(provider, "dice"),
                        result_type="Nat",
                        measure="die_face",
                    ),
                    "receipt",
                    provider,
                    positions[0].start,
                )
                return self._expression(
                    "read.captured.dice.v1", (), positions, {"value": port}
                )
            return None
        return None

    def _counter(
        self, operand: str, start: int, positions: tuple[Span, ...]
    ) -> str | None:
        counter = re.fullmatch(
            r"これ(?P<previous>に置かれていた|の)(?P<counter>[^「」]+カウンター)の数",
            operand,
        )
        if counter:
            span = Span(
                start=start + counter.end("counter") - len("カウンター"),
                end=start + counter.end("counter"),
            )
            counter_ref = self._term("rule_term", span)
            if counter_ref is None:
                self.pending.add("a1_missing_counter_concept")
                return None
            arg = self._scalar("counter", counter_ref, span)
            holder = Span(start=start, end=start + len("これ"))
            previous = counter["previous"] == "に置かれていた"
            port = self._port(
                "read_holder",
                ObjectSetRef(
                    kind="ObjectSet",
                    objects=(
                        ObjectGeneration(
                            object_id=self._identity(holder, "holder"), generation=0
                        ),
                    ),
                    origin="holder",
                ),
                "lki" if previous else "live",
                holder,
                positions[0].start,
            )
            return self._expression(
                "read.holder.previous_counter.v1"
                if previous
                else "read.object.counter.v1",
                (arg,),
                positions,
                {"subject": port},
            )
        return None

    def _receipt(
        self, operand: str, start: int, positions: tuple[Span, ...]
    ) -> str | None:
        """Processing-result reads retain their producing operation and required filters."""
        patterns = (
            (
                r"これによって墓場に置いた(?P<noun>スペル)の枚数",
                "moved_spell",
                r"[^。:]*墓場に置(?:く|き[、。])",
            ),
            (
                r"これによって墓場に置いた(?P<noun>[^・]+・カード)の枚数",
                "moved_trait_card",
                r"[^。:]*墓場に置(?:く|き[、。])",
            ),
            (
                r"これによって墓場に置いた元のコストが指定した数と同じ(?P<noun>カード)の枚数",
                "moved_matching_cost",
                r"[^。:]*墓場に置(?:く|き[、。])",
            ),
            (
                r"これによって公開した元のコスト(?P<cost>N)の(?P<noun>カード)の枚数",
                "revealed_cost",
                r"[^。:]*公開(?:する|し[、。])",
            ),
            (r"戻した枚数", "returned", r"[^。:]*戻(?:す|し[、。])"),
            (
                r"これによって破壊した(?P<noun>アミュレット)の数",
                "destroyed_amulet",
                r"[^。:]*破壊(?:する|し[、。])",
            ),
            (r"これによって捨てた手札の枚数", "discarded", r"[^。:]*捨て(?:る|[、。])"),
            (
                r"アクトした(?P<noun>『X』)の枚数",
                "acted_name",
                r"[^。:]*\{アクト\}[^。:]*:",
            ),
            (
                r"これの追加コストとしてアクトした(?P<noun>[^・]+・カード)の枚数",
                "acted_trait_card",
                r"[^。:]*追加コストとして[^。:]*アクトしてよい。",
            ),
            (
                r"取った(?P<counter>[^「」]+カウンター)の数",
                "removed_counter",
                r"[^。:]*取(?:る|り[、。])",
            ),
        )
        matched = next(
            (
                (match, suffix, producer)
                for pattern, suffix, producer in patterns
                if (match := re.fullmatch(pattern, operand))
            ),
            None,
        )
        if matched is None:
            return None
        match, suffix, producer = matched
        provider = self._provider(producer, positions[0].start)
        external = (
            self._additional_cost(match["noun"], positions[0].start)
            if provider is None and suffix == "acted_trait_card"
            else None
        )
        if provider is None and external is None:
            return None
        args = (
            self._filters(match["noun"], start + match.start("noun"))
            if "noun" in match.groupdict()
            else {}
        )
        if args is None:
            return None
        if "cost" in match.groupdict():
            span = Span(
                start=start + match.start("cost"), end=start + match.end("cost")
            )
            args["cost"] = self._scalar("cost", int(self._spelling(span)), span)
        if "counter" in match.groupdict():
            span = Span(
                start=start + match.end("counter") - len("カウンター"),
                end=start + match.end("counter"),
            )
            value = self._term("rule_term", span)
            if value is None:
                self.pending.add("a1_missing_counter_concept")
                return None
            args["counter"] = self._scalar("counter", value, span)
        identifier = "read.result." + suffix + ".v1"
        rule = REGISTRY.descriptors[identifier]
        port = external
        if provider is not None:
            port = self._port(
                "read_receipt",
                ReceiptRef(
                    kind="ReceiptId",
                    receipt_id=self._identity(provider, "receipt"),
                    result="amount" if suffix == "removed_counter" else "objects",
                ),
                "receipt",
                provider,
                positions[0].start,
            )
        assert port is not None
        return self._expression(
            identifier,
            tuple(args[p.name] for p in rule.parameters),
            positions,
            {"receipt": port},
        )

    def _additional_cost(self, noun: str, consumer: int) -> str | None:
        pattern = re.compile(
            r"これをプレイする際、追加コストとして場の(?P<noun>[^。]+)を好きな枚数アクトしてよい。"
        )
        current_start = self.original.source_span.segments[0].start
        matches = [
            (part, m)
            for part in normalize_source(self.raw, self.source).parts
            if part.source_span.role == "body"
            for m in (pattern.fullmatch(part.canonical_source),)
            if m is not None
            and m["noun"] == noun
            and part.source_span.segments[-1].end <= current_start
        ]
        if len(matches) != 1:
            return None
        part, match = matches[0]
        segments = origins(part, Span(start=match.start(), end=match.end()))
        receipt_id = "source-extra-cost:" + hash_payload(
            {
                "source": source_identity(self.source),
                "segments": [s.model_dump(mode="json") for s in segments],
            }
        )
        name = f"read_port_{len(self.ports)}"
        self.ports.append(
            Port(
                name=name,
                type="ReceiptId",
                source_role="read_receipt",
                scope=self._scope(consumer),
                evidence=PortEvidence(
                    source=OccurrenceKey(
                        owner=self.source.owner,
                        field=self.source.field,
                        ordinal=self.source.ordinal,
                        source_hash=self.source.source_hash,
                        line_ordinal=part.line_ordinal,
                        role=part.source_span.role,
                        segments=segments,
                    ),
                    capture="receipt",
                    value=ReceiptRef(
                        kind="ReceiptId", receipt_id=receipt_id, result="objects"
                    ),
                ),
            )
        )
        self.pending.discard("a1_missing_object_provider")
        return name

    def collect(self) -> None:
        """X definitions and all their permitted uses are resolved within one source scope."""
        quotes = tuple(re.finditer(r"『[^』]*』", self._text))
        self._definitions(quotes)
        self._quoted_reads()
        self._payments(quotes)

    def _definitions(self, quotes: tuple[re.Match[str], ...]) -> None:
        for match in _DEFINE.finditer(self._text):
            if any(
                getattr(v, "kind", None) == "expression"
                for v in self.found.values.values()
            ):
                continue
            scope = self._region(match.start())
            definitions = [
                m
                for m in _DEFINE.finditer(self._text)
                if self._scope(m.start()) == scope.id
            ]
            uses = [
                m
                for m in _X.finditer(self._text, scope.start, scope.end)
                if self._scope(m.start()) == scope.id
                and m.start() != match.start()
                and not any(q.start() <= m.start() < q.end() for q in quotes)
            ]
            if (
                len(definitions) != 1
                or not uses
                or any(not self._supported_x(m.start()) for m in uses)
            ):
                continue
            positions = (
                *(Span(start=m.start(), end=m.end()) for m in uses),
                Span(start=match.start(), end=match.start() + 1),
            )
            result = self.read(match["operand"], match.start("operand"), positions)
            if result is not None:
                for use in uses:
                    threshold = re.match(
                        r"Xが(?P<value>N)以上なら、", self._text[use.start() :]
                    )
                    if threshold:
                        span = Span(
                            start=use.start() + threshold.start("value"),
                            end=use.start() + threshold.end("value"),
                        )
                        self._scalar("offset", int(self._spelling(span)), span)
                        item = self.leaves[-1]
                        self.leaves[-1] = replace(
                            item,
                            slot=item.slot.model_copy(
                                update={"role": "comparison_threshold"}
                            ),
                        )

    def _quoted_reads(self) -> None:
        for match in _QUOTE.finditer(self._text):
            if not self._supported_quote(match):
                continue
            if (
                match["factor"]
                and int(
                    self._spelling(
                        Span(start=match.start("factor"), end=match.end("factor"))
                    )
                )
                < 1
            ):
                continue
            value = self._quote_value(match)
            if value is None:
                continue
            decrease = re.match(r"だけ-(?P<value>N)する。", self._text[match.end() :])
            if decrease:
                span = Span(
                    start=match.end() + decrease.start("value"),
                    end=match.end() + decrease.end("value"),
                )
                self._scalar("offset", int(self._spelling(span)), span)
                item = self.leaves[-1]
                self.leaves[-1] = replace(
                    item,
                    slot=item.slot.model_copy(update={"role": "cost_reduction_unit"}),
                )
            if match["factor"]:
                factor_span = Span(start=match.start("factor"), end=match.end("factor"))
                factor = int(self._spelling(factor_span))
                if factor < 1:
                    continue
                factor_arg = self._scalar_factor(factor, factor_span)
                self._expression(
                    "quantity.scale.once.v1",
                    (value, factor_arg),
                    (Span(start=match.start(), end=match.end()),),
                )

    def _quote_value(self, match: re.Match[str]) -> str | None:
        checkpoint = len(self.leaves), len(self.ports)
        span = Span(start=match.start("operand"), end=match.end("operand"))
        name = self.read(match["operand"], match.start("operand"), (span,))
        if name is not None and match["factor"]:
            value = next(item.value for item in self.leaves if item.slot.name == name)
            assert isinstance(value, (Expression, ContextExpression))
            scalable = (
                DEFINITIONS[value.expression_id].operator is None
                if isinstance(value, Expression)
                else REGISTRY.definition(value).scalable
            )
            if not scalable:
                del self.leaves[checkpoint[0] :]
                del self.ports[checkpoint[1] :]
                return None
        return name

    def _scalar_factor(self, value: int, span: Span) -> str:
        name = self._scalar("offset", value, span)
        item = self.leaves[-1]
        self.leaves[-1] = replace(
            item, slot=item.slot.model_copy(update={"role": "constant_multiplier"})
        )
        return name

    def _sentence(self, position: int) -> str:
        region = self._region(position)
        start = self._text.rfind("。", region.start, position) + 1
        end = self._text.find("。", position, region.end)
        return self._text[max(start, region.start) : end if end >= 0 else region.end]

    def _supported_x(self, position: int) -> bool:
        before, after = self._text[:position], self._text[position + 1 :]
        return (
            bool(
                re.search(
                    r"\{(?:攻撃力|体力)\}[+-]$|コストを-$|(?:元の)?コスト$", before
                )
                or re.match(
                    r"ダメージ。|枚引く。|回復する。|枚まで|枚を|体を|がN以上なら、",
                    after,
                )
            )
            and "割り振" not in after.split("。", 1)[0]
        )

    def _supported_quote(self, match: re.Match[str]) -> bool:
        before, after = self._text[: match.start()], self._text[match.end() :]
        return bool(
            re.match(
                r"(?:の)?ダメージ|だけ(?:\{[^{}]+\}を)?出す|だけ\{?体力|だけ回復|だけ少なくなる|だけ-Nする|枚",
                after,
            )
            or re.search(r"コストを-$|\{体力\}\+$", before)
        ) and not after.startswith("のN倍")

    def _payments(self, quotes: tuple[re.Match[str], ...]) -> None:
        """Declared X is distinct from actual receipts and payment completion."""
        for region in regions(self._text):
            text = self._text[region.start : region.end]
            if "Xは「" in text or ":" not in text:
                continue
            colon = region.start + text.index(":")
            cost = self._text[region.start : colon]
            if not (
                "{コストX}" in cost
                or re.search(
                    r"場の(?:『X』|アミュレット)X枚を(?:消滅|\{アクト\})", cost
                )
                or re.search(r"これの[^。]+カウンターX個を取る", cost)
            ):
                continue
            uses = [
                Span(start=m.start(), end=m.end())
                for m in _X.finditer(self._text, region.start, region.end)
                if self._scope(m.start()) == region.id
                and self._scope(m.end() - 1) == region.id
                and not any(q.start() <= m.start() < q.end() for q in quotes)
            ]
            if not uses or not any(s.start < colon for s in uses):
                continue
            provider = Span(start=region.start, end=colon)
            port = self._port(
                "declared_x",
                CapturedValueRef(
                    kind="CapturedValue",
                    binding_id=self._identity(provider, "declared-x"),
                    result_type="Nat",
                    measure="declared_x",
                ),
                "play_parameter",
                provider,
                uses[0].start,
            )
            self._expression(CHOSEN_X_EXPRESSION, (), tuple(uses), {"value": port})
            if "下記からX個までチョイスする。" in text:
                labels = tuple(_MODE.finditer(self._text, colon, region.end))
                values = [
                    int(self._spelling(Span(start=m.start() + 1, end=m.end() - 1)))
                    for m in labels
                ]
                if (
                    values == list(range(1, len(values) + 1))
                    and len(values) >= _PAIR_SIZE
                ):
                    for m, value in zip(labels, values, strict=True):
                        span = Span(start=m.start() + 1, end=m.end() - 1)
                        name = self._scalar("offset", value, span)
                        item = self.leaves[-1]
                        self.leaves[-1] = replace(
                            item,
                            slot=item.slot.model_copy(
                                update={
                                    "name": name,
                                    "role": "choice_index",
                                    "type": "Ordinal",
                                    "domain": Domain(
                                        values=(), min=1, max=SAFE_INTEGER
                                    ),
                                }
                            ),
                        )
            self._port(
                "payment_complete",
                ReceiptRef(
                    kind="ReceiptId",
                    receipt_id=self._identity(provider, "payment"),
                    result="payment_complete",
                ),
                "receipt",
                provider,
                uses[0].start,
            )
