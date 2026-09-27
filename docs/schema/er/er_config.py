"""Load diagram.toml: diagram groups and snapshot reference hints."""

import tomllib
from dataclasses import dataclass
from typing import TYPE_CHECKING

from er_model import LAYERS, Diagnostics, Group, LayerName

if TYPE_CHECKING:
    from pathlib import Path

LAYER_PREFIX: dict[LayerName, str] = {"build": "建置", "snapshot": "快照"}


@dataclass(slots=True)
class DiagramConfig:
    """Groups per layer and the snapshot `*_id` aliases."""

    groups: dict[LayerName, list[Group]]
    snapshot_aliases: dict[str, str]


def _table(value: object, where: str) -> dict[str, object]:
    if not isinstance(value, dict):
        msg = f"diagram.toml: {where} 應為 table"
        raise TypeError(msg)
    return {str(k): v for k, v in value.items()}


def _str_list(value: object, where: str) -> list[str]:
    if not isinstance(value, list) or not all(isinstance(x, str) for x in value):
        msg = f"diagram.toml: {where} 應為字串陣列"
        raise TypeError(msg)
    return [str(x) for x in value]


def _groups(layer: LayerName, value: object) -> list[Group]:
    if not isinstance(value, list):
        msg = f"diagram.toml: {layer}.groups 應為 array of tables"
        raise TypeError(msg)
    out: list[Group] = []
    for i, item in enumerate(value):
        g = _table(item, f"{layer}.groups[{i}]")
        key, title = g.get("key"), g.get("title")
        if not isinstance(key, str) or not isinstance(title, str):
            msg = f"diagram.toml: {layer}.groups[{i}] 需要字串 key 與 title"
            raise TypeError(msg)
        tables = _str_list(g.get("tables"), f"{layer}.groups[{i}].tables")
        out.append(Group(key, f"{LAYER_PREFIX[layer]}：{title}", title, tables))
    return out


def load_config(path: Path) -> DiagramConfig:
    """Read and type-check diagram.toml."""
    with path.open("rb") as f:
        data = tomllib.load(f)
    groups: dict[LayerName, list[Group]] = {}
    for layer in LAYERS:
        groups[layer] = _groups(layer, _table(data.get(layer), layer).get("groups"))
    refs = _table(
        _table(data.get("snapshot"), "snapshot").get("references", {}),
        "snapshot.references",
    )
    aliases: dict[str, str] = {}
    for k, v in refs.items():
        if not isinstance(v, str):
            msg = f"diagram.toml: snapshot.references.{k} 應為字串"
            raise TypeError(msg)
        aliases[k] = v
    return DiagramConfig(groups, aliases)


def check_groups(
    layer: LayerName, groups: list[Group], tables: set[str], diag: Diagnostics
) -> None:
    """Every table in exactly one group, and no group names a table that does not exist."""
    owner: dict[str, str] = {}
    keys: set[str] = set()
    for g in groups:
        if g.key in keys:
            diag.errors.append(f"{layer} 分組鍵 {g.key} 重複")
        keys.add(g.key)
        for n in g.tables:
            if n not in tables:
                diag.errors.append(f"{layer} 分組 {g.key} 含不存在的表 {n}")
            elif n in owner:
                diag.errors.append(f"{layer} 表 {n} 同時屬於 {owner[n]} 與 {g.key}")
            else:
                owner[n] = g.key
    if missing := sorted(tables - owner.keys()):
        diag.errors.append(f"{layer} 未分組的表：{missing}")


def check_aliases(aliases: dict[str, str], tables: set[str], diag: Diagnostics) -> None:
    """Alias targets must be real snapshot collections (or empty)."""
    for k, v in aliases.items():
        if v and v not in tables:
            diag.errors.append(
                f"diagram.toml snapshot.references.{k} 指向不存在的集合 {v}"
            )
