"""Data types shared by the schema ER page generator."""

import json
from dataclasses import asdict, dataclass, field
from typing import TYPE_CHECKING, Literal

if TYPE_CHECKING:
    from pathlib import Path

type LayerName = Literal["build", "snapshot"]
type FkVia = Literal["decl", "constraint", "inferred"]

LAYERS: tuple[LayerName, ...] = ("build", "snapshot")


@dataclass(slots=True)
class RawTable:
    """One Markdown table row: a table name with its declaration and notes, untouched."""

    name: str
    declaration: str
    constraints: str
    source: str
    line: int


@dataclass(slots=True)
class SchemaModel:
    """Everything the generator reads from the schema documents."""

    sources: dict[str, str]
    build: list[RawTable]
    snapshot: list[RawTable]
    nested_types: list[RawTable]

    def layer(self, name: LayerName) -> list[RawTable]:
        """Return the raw tables of one layer."""
        return self.build if name == "build" else self.snapshot

    def dump(self, path: Path) -> None:
        """Write the model as JSON for diagnosis."""
        path.write_text(
            json.dumps(asdict(self), ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )

    @classmethod
    def load(cls, path: Path) -> SchemaModel:
        """Read a model written by `dump`, checking every field."""
        data = _as_dict(json.loads(path.read_text(encoding="utf-8")), "model")
        sources = _as_dict(data.get("sources"), "sources")
        return cls(
            sources={k: _as_str(v, f"sources.{k}") for k, v in sources.items()},
            build=_raw_tables(data.get("build"), "build"),
            snapshot=_raw_tables(data.get("snapshot"), "snapshot"),
            nested_types=_raw_tables(data.get("nested_types"), "nested_types"),
        )


@dataclass(slots=True)
class Column:
    """A parsed column; field names are what template.html reads."""

    name: str
    type: str = ""
    nullable: bool = False
    pk: bool = False
    fk_target: str | None = None
    fk_cols: list[str] | None = None
    fk_via: FkVia | None = None
    uq: list[str] = field(default_factory=list[str])
    enum_values: list[str] | None = None
    raw: str | None = None
    nested_refs: list[str] = field(default_factory=list[str])
    constraint_fks: list[str] = field(default_factory=list[str])


@dataclass(slots=True)
class Relation:
    """A drawn edge from a referencing column group to a referenced table."""

    source: str
    cols: list[str]
    target: str
    target_cols: list[str] | None
    nullable: bool
    via: FkVia
    label: str | None = None

    def to_json(self) -> dict[str, object]:
        """Serialise with the keys template.html expects."""
        return {
            "from": self.source,
            "col": self.cols[0],
            "cols": self.cols,
            "to": self.target,
            "to_col": self.target_cols[0] if self.target_cols else None,
            "to_cols": self.target_cols,
            "nullable": self.nullable,
            "via": self.via,
            "label": self.label,
        }


@dataclass(slots=True)
class Table:
    """A parsed table or snapshot collection."""

    name: str
    cols: list[Column]
    constraints: str
    line: int
    source: str
    nested_refs: list[str]


@dataclass(slots=True)
class Group:
    """A diagram group of one layer."""

    key: str
    title: str
    short: str
    tables: list[str]


@dataclass(slots=True)
class Diagnostics:
    """Problems found while parsing; anything in `errors` or `fk_missing` fails the run."""

    errors: list[str] = field(default_factory=list[str])
    fk_missing: list[str] = field(default_factory=list[str])
    notes: list[str] = field(default_factory=list[str])

    @property
    def failed(self) -> bool:
        """Whether the page must not be published."""
        return bool(self.errors or self.fk_missing)


def _as_dict(value: object, where: str) -> dict[str, object]:
    if not isinstance(value, dict):
        msg = f"{where}: expected an object"
        raise TypeError(msg)
    out: dict[str, object] = {}
    for k, v in value.items():
        out[_as_str(k, where)] = v
    return out


def _as_str(value: object, where: str) -> str:
    if not isinstance(value, str):
        msg = f"{where}: expected a string"
        raise TypeError(msg)
    return value


def _raw_tables(value: object, where: str) -> list[RawTable]:
    if not isinstance(value, list):
        msg = f"{where}: expected a list"
        raise TypeError(msg)
    out: list[RawTable] = []
    for i, item in enumerate(value):
        d = _as_dict(item, f"{where}[{i}]")
        line = d.get("line")
        if not isinstance(line, int):
            msg = f"{where}[{i}].line: expected an integer"
            raise TypeError(msg)
        out.append(
            RawTable(
                name=_as_str(d.get("name"), f"{where}[{i}].name"),
                declaration=_as_str(d.get("declaration"), f"{where}[{i}].declaration"),
                constraints=_as_str(d.get("constraints"), f"{where}[{i}].constraints"),
                source=_as_str(d.get("source"), f"{where}[{i}].source"),
                line=line,
            )
        )
    return out
