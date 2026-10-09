"""Read table declarations out of the schema Markdown documents."""

import hashlib
import re
from dataclasses import dataclass
from typing import TYPE_CHECKING

from er_model import Diagnostics, RawTable, SchemaModel

if TYPE_CHECKING:
    from pathlib import Path

BUILD_DOC = "build/build-db.md"
SNAPSHOT_DOC = "export/snapshot-format.md"

# Header cells that mark the tables holding declarations; other tables in the documents are prose.
BUILD_HEADER = ("表", "建置期欄位、鍵與約束")
SNAPSHOT_HEADER = ("集合", "公開欄位")
NESTED_HEADER = "巢狀型別"
# The snapshot generator relies on this stated default; if the wording goes, so must the rule.
SNAPSHOT_PK_RULE = "未註明 PK 者以首欄 `id` 為 PK"

NAME_RE = re.compile(r"[a-z_][a-z0-9_]*")
NESTED_NAME_RE = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")
_CELL_SPLIT = re.compile(r"(?<!\\)\|")
_SEPARATOR = re.compile(r"\|(\s*:?-+:?\s*\|)+")
_LEADING_CODE = re.compile(r"`([^`]+)`(.*)", re.DOTALL)


@dataclass(slots=True)
class MdTable:
    """A GitHub-flavoured Markdown table with the 1-based line number of each row."""

    header: list[str]
    rows: list[tuple[int, list[str]]]


def split_row(line: str) -> list[str]:
    r"""Split a table row into cells, unescaping `\|` inside cells."""
    cells = _CELL_SPLIT.split(line.strip())
    return [c.strip().replace("\\|", "|") for c in cells[1:-1]]


def iter_tables(text: str) -> list[MdTable]:
    """Return every Markdown table in `text`."""
    lines = text.splitlines()
    tables: list[MdTable] = []
    i = 0
    while i < len(lines) - 1:
        if lines[i].startswith("|") and _SEPARATOR.fullmatch(lines[i + 1].strip()):
            table = MdTable(header=split_row(lines[i]), rows=[])
            i += 2
            while i < len(lines) and lines[i].startswith("|"):
                table.rows.append((i + 1, split_row(lines[i])))
                i += 1
            tables.append(table)
        else:
            i += 1
    return tables


def _unquote(cell: str) -> str:
    return cell.strip().strip("`")


def _read_build(path: Path, source: str, diag: Diagnostics) -> list[RawTable]:
    out: list[RawTable] = []
    for table in iter_tables(path.read_text(encoding="utf-8")):
        if tuple(table.header[:2]) != BUILD_HEADER:
            continue
        for line, cells in table.rows:
            where = f"{source}:{line}"
            name = _unquote(cells[0]) if cells else ""
            m = (
                _LEADING_CODE.fullmatch(cells[1])
                if len(cells) == len(BUILD_HEADER)
                else None
            )
            if not NAME_RE.fullmatch(name) or m is None:
                diag.errors.append(
                    f"{where}: 讀不出表名與宣告（應為「`表名` | `欄位宣告`約束」）"
                )
                continue
            out.append(RawTable(name, m.group(1), m.group(2).strip(), source, line))
    return out


def _read_snapshot(
    path: Path, source: str, diag: Diagnostics
) -> tuple[list[RawTable], list[RawTable]]:
    collections: list[RawTable] = []
    nested: list[RawTable] = []
    for table in iter_tables(path.read_text(encoding="utf-8")):
        if tuple(table.header[:2]) == SNAPSHOT_HEADER:
            if (
                len(table.header) < len(SNAPSHOT_HEADER) + 1
                or SNAPSHOT_PK_RULE not in table.header[2]
            ):
                diag.errors.append(
                    f"{source}: 集合表頭沒有寫「{SNAPSHOT_PK_RULE}」，預設 PK 規則不能套用"
                )
            collections.extend(_read_rows(source, table, NAME_RE, diag))
        elif table.header[:1] == [NESTED_HEADER]:
            nested.extend(_read_rows(source, table, NESTED_NAME_RE, diag))
    return collections, nested


def _read_rows(
    source: str, table: MdTable, name_re: re.Pattern[str], diag: Diagnostics
) -> list[RawTable]:
    out: list[RawTable] = []
    for line, cells in table.rows:
        where = f"{source}:{line}"
        name = _unquote(cells[0]) if cells else ""
        m = (
            _LEADING_CODE.fullmatch(cells[1])
            if len(cells) >= len(SNAPSHOT_HEADER)
            else None
        )
        if not name_re.fullmatch(name) or m is None:
            diag.errors.append(f"{where}: 讀不出名稱與欄位（應為「`名稱` | `欄位`」）")
            continue
        rest = m.group(2).strip()
        # Collections keep their key notes in the third cell; nested types describe semantics after the fields.
        notes = (
            " | ".join(c for c in cells[2:] if c)
            if len(cells) > len(SNAPSHOT_HEADER)
            else ""
        )
        if rest and notes:
            diag.errors.append(f"{where}: 欄位格在宣告之後還有文字 `{rest}`")
        out.append(RawTable(name, m.group(1), notes or rest.lstrip("；"), source, line))
    return out


def _check_unique(tables: list[RawTable], label: str, diag: Diagnostics) -> None:
    seen: dict[str, RawTable] = {}
    for t in tables:
        if t.name in seen:
            first = seen[t.name]
            diag.errors.append(
                f"{label} 重複宣告 {t.name}（{first.source}:{first.line} 與 {t.source}:{t.line}）"
            )
        seen[t.name] = t


def read_model(schema_dir: Path, diag: Diagnostics) -> SchemaModel:
    """Extract the raw declarations of both layers from the schema documents."""
    build_path = schema_dir / BUILD_DOC
    snapshot_path = schema_dir / SNAPSHOT_DOC
    build = _read_build(build_path, BUILD_DOC, diag)
    snapshot, nested = _read_snapshot(snapshot_path, SNAPSHOT_DOC, diag)
    _check_unique(build, "建置資料庫", diag)
    _check_unique(snapshot, "卡表快照", diag)
    _check_unique(nested, "巢狀型別", diag)
    return SchemaModel(
        sources=source_hashes(schema_dir),
        build=build,
        snapshot=snapshot,
        nested_types=nested,
    )


def source_hashes(schema_dir: Path) -> dict[str, str]:
    """SHA-256 of each source document, so a saved model can be matched to the docs it came from."""
    return {
        name: hashlib.sha256((schema_dir / name).read_bytes()).hexdigest()
        for name in (BUILD_DOC, SNAPSHOT_DOC)
    }
