"""Pure import graph consistency, without source admission or execution ordering."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.domains.effects.author_validation import PathParts, Violation

if TYPE_CHECKING:
    from collections.abc import Sequence

    from sve_carddb.contracts.author_evidence import (
        ImportMap,
        InterfaceDependency,
        SourceUse,
    )
    from sve_carddb.contracts.four_layer import Port

type UseKey = tuple[str, str]


@dataclass(frozen=True)
class ImportNode:
    source: SourceUse
    interface: InterfaceDependency
    imports: ImportMap
    exports: tuple[Port, ...]
    path: PathParts = ()
    input_ports: tuple[Port, ...] = ()

    @property
    def key(self) -> UseKey:
        """The source binding and ability scope jointly identify one use."""
        return self.source.binding_id, self.source.ability_scope


@dataclass(frozen=True)
class ImportClosure:
    key: UseKey
    interfaces: tuple[InterfaceDependency, ...]


@dataclass(frozen=True)
class ImportAnalysis:
    order: tuple[UseKey, ...]
    closures: tuple[ImportClosure, ...]
    violations: tuple[Violation, ...]


class _Graph:
    def __init__(self, nodes: dict[UseKey, ImportNode]) -> None:
        self.nodes = nodes
        self.failures: list[Violation] = []
        self.active: set[UseKey] = set()
        self.completed: dict[UseKey, dict[str, InterfaceDependency]] = {}
        self.order: list[UseKey] = []

    def visit(self, key: UseKey) -> dict[str, InterfaceDependency]:
        if key in self.completed:
            return self.completed[key]
        node = self.nodes[key]
        if key in self.active:
            self.failures.append(Violation("cyclic_author_import", node.path))
            return {}
        self.active.add(key)
        interfaces = {node.interface.frame_id: node.interface}
        for name, imported in sorted(node.imports.items()):
            path = (*node.path, "imports", name)
            upstream = self.nodes.get((imported.binding_id, imported.ability_scope))
            if upstream is None:
                self.failures.append(Violation("missing_import_use", path))
                continue
            ports = [port for port in upstream.exports if port.name == imported.port]
            if len(ports) != 1:
                self.failures.append(Violation("missing_or_ambiguous_export", path))
                continue
            self.evidence(node, name, ports[0], upstream, path)
            for frame, interface in self.visit(upstream.key).items():
                if frame in interfaces and interfaces[frame] != interface:
                    self.failures.append(
                        Violation("conflicting_import_interface", path)
                    )
                interfaces[frame] = interface
        self.active.remove(key)
        self.completed[key] = interfaces
        self.order.append(key)
        return interfaces

    def evidence(
        self,
        node: ImportNode,
        name: str,
        export: Port,
        upstream: ImportNode,
        path: PathParts,
    ) -> None:
        ports = [port for port in node.input_ports if port.name == name]
        if len(ports) > 1:
            self.failures.append(Violation("ambiguous_import_port", path))
        for port in (export, *ports):
            if (
                port.evidence is not None
                and port.evidence.source != upstream.source.occurrence
            ):
                self.failures.append(Violation("import_occurrence_mismatch", path))


def analyze_imports(uses: Sequence[ImportNode]) -> ImportAnalysis:
    """Resolve exact use keys; successful graphs still carry no source proof."""
    nodes: dict[UseKey, ImportNode] = {}
    failures: list[Violation] = []
    for node in uses:
        if node.key in nodes:
            failures.append(Violation("ambiguous_import_use", node.path))
        nodes[node.key] = node
    if failures:
        return ImportAnalysis((), (), tuple(failures))
    graph = _Graph(nodes)
    for key in sorted(nodes):
        graph.visit(key)
    if graph.failures:
        return ImportAnalysis((), (), tuple(graph.failures))
    closures = tuple(
        ImportClosure(
            key,
            tuple(
                graph.completed[key][frame] for frame in sorted(graph.completed[key])
            ),
        )
        for key in graph.order
    )
    return ImportAnalysis(tuple(graph.order), closures, ())
