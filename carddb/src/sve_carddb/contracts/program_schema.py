"""Compile the public program's pinned DSL schemas into local snapshot definitions."""

from pathlib import Path

from pydantic import JsonValue

from sve_carddb.core.json import object_value, parse, string


class _Definitions:
    def __init__(self, directory: Path) -> None:
        self.sources = {
            prefix: object_value(parse((directory / filename).read_bytes()))
            for prefix, filename in (
                ("DSL.", "program.schema.json"),
                ("DSLNodes.", "author-nodes.schema.json"),
            )
        }
        self.result: dict[str, JsonValue] = {}
        self.pending: list[tuple[str, JsonValue, str]] = [
            ("DSLProgram", self.sources["DSL."], "DSL.")
        ]

    def reference(self, reference: str, prefix: str) -> str:
        external = "urn:sve-kit:author-nodes:1.0#/$defs/"
        if reference.startswith(external):
            name, owner = reference[len(external) :], "DSLNodes."
        elif reference.startswith("#/$defs/"):
            name, owner = reference[len("#/$defs/") :], prefix
        else:
            raise ValueError("Unsupported public program schema reference")
        target = owner + name
        if target not in self.result:
            self.pending.append(
                (target, object_value(self.sources[owner]["$defs"])[name], owner)
            )
        return "#/$defs/" + target

    def local(self, value: JsonValue, prefix: str) -> JsonValue:
        if isinstance(value, list):
            return [self.local(item, prefix) for item in value]
        if not isinstance(value, dict):
            return value
        if "$dynamicRef" in value:
            reference = string(value["$dynamicRef"])
            if reference not in {
                "urn:sve-kit:author-nodes:1.0:authorParam#authorParam",
                "urn:sve-kit:author-nodes:1.0:authorGenerated#authorGenerated",
            }:
                raise ValueError("Unknown expanded program dynamic reference")
            return {"not": {}}
        return {
            key: self.reference(string(item), prefix)
            if key == "$ref"
            else self.local(item, prefix)
            for key, item in value.items()
            if key not in {"$id", "$schema", "$defs", "$dynamicAnchor"}
        }

    def compile(self) -> dict[str, JsonValue]:
        while self.pending:
            name, value, prefix = self.pending.pop()
            if name not in self.result:
                # Recursive nodes share one definition in the emitted resource.
                self.result[name] = {}
                self.result[name] = self.local(value, prefix)
        return self.result


def definitions() -> dict[str, JsonValue]:
    """Read authority resources only while regenerating the checked-in wire schema."""
    return _Definitions(Path(__file__).resolve().parents[4] / "dsl").compile()
