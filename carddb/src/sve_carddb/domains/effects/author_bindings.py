"""Analyze local bindings over schema sites, without proving source port semantics."""

from collections import defaultdict
from dataclasses import dataclass
from typing import TYPE_CHECKING, get_args

from pydantic import ConfigDict, JsonValue, TypeAdapter

from sve_carddb.contracts.author_programs import AbilityHead
from sve_carddb.contracts.author_references import ReservedPlayerName, ReservedRefName
from sve_carddb.domains.effects.author_validation import PathParts, Violation

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.contracts.author_evidence import Ports
    from sve_carddb.domains.effects.author_sites import NodeSites

_HEAD = TypeAdapter(
    AbilityHead, config=ConfigDict(strict=True, regex_engine="python-re")
)
_RESERVED = frozenset((*get_args(ReservedPlayerName), *get_args(ReservedRefName)))
_STRING_READS = {
    "A.ref": frozenset({"ObjectSet", "ObjectRef", "RecipientSet"}),
    "A.player-ref": frozenset({"PlayerRef"}),
    "A.follow-source": frozenset({"ReceiptId", "ReceiptList"}),
    "A.receipt-ref": frozenset({"ReceiptId", "ReceiptList"}),
    "A.expr.numeric-operation": frozenset({"Number", "CapturedValue", "QuantityExpr"}),
}
_FIELD_READS = {
    "A.expr.bound": ("bound", frozenset({"Number", "CapturedValue", "QuantityExpr"})),
    "A.role-selection": ("of", frozenset({"ObjectSet", "RecipientSet"})),
    "A.capture.selection": ("selection", frozenset({"ObjectSet", "RecipientSet"})),
    "A.position.chosen": ("chosen", frozenset({"PositionChoice"})),
    "A.x.payment": ("from_payment", frozenset({"ReceiptId"})),
    "A.expr.dice": ("dice", frozenset({"Number"})),
    "A.expr.x-paid": ("x_paid", frozenset({"ReceiptId"})),
    "A.chosen-payload": ("ref", frozenset({"ObjectSet", "RecipientSet"})),
}
_SET_PRODUCERS = frozenset(
    {
        "A.act.choose.exact",
        "A.act.choose.up-to",
        "A.act.choose.any",
        "A.act.look.ref",
        "A.act.look.top",
        "A.act.reveal.ref",
        "A.act.reveal.top",
        "A.act.reveal-until",
        "A.act.create",
        "A.target.grouped.exact",
        "A.target.grouped.up-to",
        "A.target.grouped.any",
    }
)
_TARGET_PRODUCERS = frozenset({"A.target.exact", "A.target.up-to", "A.target.any"})


@dataclass(frozen=True)
class Declaration:
    path: PathParts
    name: str
    type: str
    scope: PathParts
    execution: PathParts


@dataclass(frozen=True)
class BindingAnalysis:
    declarations: tuple[Declaration, ...]
    violations: tuple[Violation, ...]


class _Walker:
    def __init__(self, tree: JsonValue, sites: NodeSites) -> None:
        self.tree = tree
        self.definitions: dict[PathParts, set[str]] = defaultdict(set)
        for node in sites.walk(tree):
            self.definitions[node.path].add(node.definition)
        self.declarations: list[Declaration] = []
        self.failures: list[Violation] = []
        self.aliases: dict[tuple[PathParts, str], PathParts] = {}
        self.expanded: set[tuple[PathParts, str]] = set()
        self.reading: set[tuple[PathParts, str]] = set()
        self.handled: set[PathParts] = set()

    def value(self, path: PathParts) -> JsonValue:
        node = self.tree
        for part in path:
            if isinstance(node, dict) and isinstance(part, str):
                node = node[part]
                continue
            if isinstance(node, list) and isinstance(part, int):
                node = node[part]
                continue
            raise TypeError("Invalid validated syntax path")
        return node

    def declare(
        self,
        name: str,
        kind: str,
        path: PathParts,
        env: dict[str, Declaration],
        scope: PathParts,
        execution: PathParts,
    ) -> None:
        if name in env and env[name].scope == scope:
            self.failures.append(Violation("duplicate_local_binding", path))
            return
        declaration = Declaration(path, name, kind, scope, execution)
        self.declarations.append(declaration)
        env[name] = declaration

    def reference(
        self,
        name: str,
        kinds: frozenset[str],
        path: PathParts,
        env: dict[str, Declaration],
        scope: PathParts,
        execution: PathParts,
    ) -> None:
        if name in _RESERVED and name not in env:
            return
        declaration = env.get(name)
        if declaration is None:
            siblings = [
                item
                for item in self.declarations
                if item.name == name
                and item.execution == execution
                and item.scope != scope
            ]
            reason = (
                "unsupported_binding_merge"
                if len(siblings) > 1
                else "binding_not_visible"
            )
            self.failures.append(Violation(reason, path))
            return
        if declaration.type not in kinds:
            reason = (
                "unsupported_import_binding_type"
                if declaration.type == "Unknown"
                else "binding_type_mismatch"
            )
            self.failures.append(Violation(reason, path))
            return
        key = execution, name
        if key in self.aliases:
            if key in self.reading:
                self.failures.append(Violation("cyclic_binding_alias", path))
                return
            self.reading.add(key)
            self.expanded.add(key)
            self.visit(self.aliases[key], env, scope, execution)
            self.reading.remove(key)

    def read(
        self,
        path: PathParts,
        env: dict[str, Declaration],
        scope: PathParts,
        execution: PathParts,
    ) -> bool:
        value = self.value(path)
        definitions = self.definitions[path]
        for definition, kinds in _STRING_READS.items():
            if definition in definitions and isinstance(value, str):
                self.reference(value, kinds, path, env, scope, execution)
                return True
        for definition, (field, kinds) in _FIELD_READS.items():
            if definition in definitions and isinstance(value, dict):
                name = value.get(field)
                if isinstance(name, str):
                    self.handled.add((*path, field))
                    self.reference(name, kinds, (*path, field), env, scope, execution)
        return False

    def visit(
        self,
        path: PathParts,
        env: dict[str, Declaration],
        scope: PathParts,
        execution: PathParts,
    ) -> None:
        value = self.value(path)
        definitions = self.definitions[path]
        if definitions & {"A.granted-ability-head", "A.act.delayed"}:
            env, scope, execution = {}, path, path
        if self.read(path, env, scope, execution):
            return
        if isinstance(value, list):
            for index in range(len(value)):
                self.visit((*path, index), env, scope, execution)
            return
        if not isinstance(value, dict):
            if "BindingName" in definitions and path not in self.handled:
                self.failures.append(Violation("unsupported_binding_site", path))
            return
        if (
            any(name.startswith("A.head.") for name in definitions)
            or "A.act.delayed" in definitions
        ):
            self.head(path, value, env, scope, execution)
            return
        if any(name.startswith("A.control.") for name in definitions):
            self.control(path, value, definitions, env, scope, execution)
            return
        self.operation(path, value, definitions, env, scope, execution)

    def operation(
        self,
        path: PathParts,
        value: dict[str, JsonValue],
        definitions: set[str],
        env: dict[str, Declaration],
        scope: PathParts,
        execution: PathParts,
    ) -> None:
        # Replacement outputs belong to their alternative, never the outer action.
        if any(name.startswith("A.instead.") for name in definitions):
            local = env.copy()
            for field in sorted(value.keys() - {"replace_with", "after"}):
                self.visit((*path, field), local, path, execution)
            for field in ("replace_with", "after"):
                if field in value:
                    self.visit((*path, field), local, path, execution)
            return
        outputs = self.outputs(path, definitions)
        for field in sorted(value.keys() - outputs.keys()):
            self.visit((*path, field), env, scope, execution)
        for field, kind in outputs.items():
            name = value.get(field)
            if isinstance(name, str):
                self.declare(name, kind, path, env, scope, execution)

    def outputs(self, path: PathParts, definitions: set[str]) -> dict[str, str]:
        outputs: dict[str, str] = {}
        if definitions & _SET_PRODUCERS:
            outputs["bind"] = "ObjectSet"
        if definitions & _TARGET_PRODUCERS:
            source_path = (*path, "from")
            object_only = "A.sel.object-core" in self.definitions[source_path]
            outputs["bind"] = "ObjectSet" if object_only else "RecipientSet"
        if "A.act.reveal-until" in definitions:
            outputs["matched"] = "ObjectSet"
        if definitions & {"A.act.dice", "A.act.declare.number"}:
            outputs["bind"] = "Number"
        if "A.act.declare.name" in definitions:
            outputs["bind"] = "CardName"
        if any(
            name.startswith(("A.act.", "A.cost.", "A.fusion.")) for name in definitions
        ):
            outputs["result"] = "ReceiptId"
        return outputs

    def merge(self, additions: list[Declaration], env: dict[str, Declaration]) -> None:
        for item in additions:
            if item.name in env:
                self.failures.append(Violation("duplicate_local_binding", item.path))
            env[item.name] = item

    def head(
        self,
        path: PathParts,
        value: dict[str, JsonValue],
        env: dict[str, Declaration],
        scope: PathParts,
        execution: PathParts,
    ) -> None:
        definitions = value.get("define")
        if isinstance(definitions, dict):
            for name in sorted(definitions):
                self.declare(name, "Number", (*path, "define"), env, scope, execution)
                self.aliases[execution, name] = (*path, "define", name)
        phases = (
            "cost",
            "fusion",
            "optional_cost",
            "additional_cost",
            "x",
            "targets",
            "do",
            "effects",
        )
        for field in sorted(value.keys() - {"define", *phases}):
            self.visit((*path, field), env, scope, execution)
        # Parallel head fields cannot acquire a payment order from mapping keys.
        additions: list[Declaration] = []
        for field in ("cost", "fusion", "optional_cost", "additional_cost", "targets"):
            if field in value:
                local = env.copy()
                self.visit((*path, field), local, scope, execution)
                additions.extend(
                    item for name, item in local.items() if name not in env
                )
        self.merge(additions, env)
        if "x" in value:
            self.visit((*path, "x"), env, scope, execution)
            self.declare("X", "Number", (*path, "x"), env, scope, execution)
        for field in ("do", "effects"):
            if field in value:
                self.visit((*path, field), env, scope, execution)
        if isinstance(definitions, dict):
            self.unused_aliases(path, definitions, env, scope, execution)

    def unused_aliases(
        self,
        path: PathParts,
        definitions: dict[str, JsonValue],
        env: dict[str, Declaration],
        scope: PathParts,
        execution: PathParts,
    ) -> None:
        # Unused aliases still need resolvable names in their completed execution.
        for name in sorted(definitions):
            if (execution, name) not in self.expanded:
                self.reference(
                    name,
                    frozenset({"Number"}),
                    (*path, "define", name),
                    env,
                    scope,
                    execution,
                )

    def control(
        self,
        path: PathParts,
        value: dict[str, JsonValue],
        definitions: set[str],
        env: dict[str, Declaration],
        scope: PathParts,
        execution: PathParts,
    ) -> None:
        children = {"do", "then", "else", "options", "stages", "leader", "follower"}
        if definitions & {"A.control.pay", "A.control.optional"}:
            local = env.copy()
            for field in ("who", "cost", "do", "then"):
                if field in value:
                    self.visit((*path, field), local, path, execution)
            name = value.get("result")
            if isinstance(name, str):
                self.declare(name, "ReceiptId", path, env, scope, execution)
            return
        for field in sorted(
            value.keys() - children - {"as", "bind", "result", "collect"}
        ):
            self.visit((*path, field), env, scope, execution)
        branches = self.branches(path, value, definitions)
        simultaneous = "A.control.simultaneous" in definitions
        completed: list[Declaration] = []
        for branch in branches:
            local = env.copy()
            self.branch_binding(path, value, definitions, local, branch, execution)
            self.visit(branch, local, scope if simultaneous else branch, execution)
            if simultaneous:
                completed.extend(
                    item for name, item in local.items() if name not in env
                )
        self.merge(completed, env)
        name = value.get("result")
        if isinstance(name, str):
            self.declare(name, "ReceiptId", path, env, scope, execution)
        if "bind" in value or "collect" in value:
            self.failures.append(Violation("unsupported_control_binding", path))

    @staticmethod
    def branches(
        path: PathParts,
        value: dict[str, JsonValue],
        definitions: set[str],
    ) -> list[PathParts]:
        branches: list[PathParts] = []
        for field in ("do", "then", "else", "options", "stages", "leader", "follower"):
            child = value.get(field)
            if field not in value:
                continue
            split = (
                field in {"options", "stages"}
                or "A.control.simultaneous" in definitions
            )
            if split and isinstance(child, list):
                branches.extend((*path, field, index) for index in range(len(child)))
            else:
                branches.append((*path, field))
        return branches

    def branch_binding(
        self,
        path: PathParts,
        value: dict[str, JsonValue],
        definitions: set[str],
        local: dict[str, Declaration],
        branch: PathParts,
        execution: PathParts,
    ) -> None:
        name = value.get("as", "item")
        kind = None
        if "A.control.foreach" in definitions:
            kind = "ObjectRef"
        elif "A.control.each-player" in definitions:
            kind = "PlayerRef"
        elif "A.control.by-kind" in definitions:
            kind = "PlayerRef" if branch[-1] == "leader" else "ObjectRef"
        if kind is not None and isinstance(name, str):
            self.declare(name, kind, path, local, branch, execution)

    def alias_dependencies(self, path: PathParts) -> set[str]:
        dependencies: set[str] = set()
        for location, definitions in self.definitions.items():
            if location[: len(path)] != path:
                continue
            value = self.value(location)
            if "A.expr.numeric-operation" in definitions and isinstance(value, str):
                dependencies.add(value)
            if "A.expr.bound" in definitions and isinstance(value, dict):
                name = value.get("bound")
                if isinstance(name, str):
                    dependencies.add(name)
        return dependencies

    def check_aliases(self) -> None:
        active: set[tuple[PathParts, str]] = set()
        done: set[tuple[PathParts, str]] = set()

        def visit(key: tuple[PathParts, str]) -> None:
            if key in active:
                self.failures.append(
                    Violation("cyclic_binding_alias", self.aliases[key])
                )
                return
            if key in done:
                return
            active.add(key)
            for name in self.alias_dependencies(self.aliases[key]):
                target = key[0], name
                if target in self.aliases:
                    visit(target)
            active.remove(key)
            done.add(key)

        for key in self.aliases:
            visit(key)


def analyze_bindings(
    head: AbilityHead,
    sites: NodeSites,
    imports: Mapping[str, str] | None = None,
) -> BindingAnalysis:
    """Inspect a validated head; import types are claims, never source proof."""
    tree = _HEAD.dump_python(head, mode="json", by_alias=True)
    walker = _Walker(tree, sites)
    env: dict[str, Declaration] = {}
    for name, kind in sorted((imports or {}).items()):
        walker.declare(name, kind, (), env, (), ())
    walker.visit((), env, (), ())
    walker.check_aliases()
    return BindingAnalysis(
        tuple(walker.declarations), tuple(dict.fromkeys(walker.failures))
    )


def validate_author_ports(
    analysis: BindingAnalysis, ports: Ports
) -> tuple[Violation, ...]:
    """Check exact local declarations; no projection/source-role mapping is inferred."""
    failures = []
    seen: set[str] = set()
    for name, imported in ports.imports.items():
        if imported.bind in seen:
            failures.append(
                Violation("duplicate_import_binding", ("ports", "imports", name))
            )
        seen.add(imported.bind)
    for name, local in ports.exports.items():
        matches = [
            item
            for item in analysis.declarations
            if item.path == local.path and item.name == local.bind
        ]
        path = ("ports", "exports", name)
        if len(matches) != 1:
            failures.append(Violation("missing_or_ambiguous_local_export", path))
        elif matches[0].scope or matches[0].execution:
            failures.append(Violation("unsupported_local_export_scope", path))
    return tuple(failures)
