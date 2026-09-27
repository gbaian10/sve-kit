"""Turn raw declarations into columns and relations for the ER page."""

import re
from dataclasses import dataclass, field

from er_model import Column, Diagnostics, FkVia, LayerName, RawTable, Relation, Table

NAME = r"[A-Za-z_][A-Za-z0-9_]*"
_CN_NUM = {"兩": 2, "二": 2, "三": 3, "四": 4, "五": 5}
_TAIL_FLAG = re.compile(r"\s+(PK|UNIQUE)$")
_BUILD_FK = re.compile(rf"({NAME})\s*→\s*({NAME})(\?)?")
_TYPED = re.compile(rf"({NAME})\s*:\s*(.+)", re.DOTALL)
_SNAPSHOT_FIELD = re.compile(rf"({NAME})(\?)?(?:\s*:\s*(.+))?", re.DOTALL)
_ARRAY_OF_NAME = re.compile(rf"\[({NAME})\]")
_ENUM_SLASH = re.compile(r"[a-z_]+(/[a-z_]+)+")
_PK = re.compile(r"(?<![A-Za-z])PK(?:\(([^)]*)\))?")
_UQ = re.compile(r"(部分\s*)?UQ\(([^)]*)\)")
_CONSTRAINT_FK = re.compile(rf"FK\(([^)]*)\)\s*→\s*({NAME})(?:\(([^)]*)\))?")
_ARROW_REF = re.compile(rf"→\s*({NAME})")
_COLUMN_COUNT = re.compile(r"前?([兩二三四五])欄")


@dataclass(slots=True)
class SnapshotRefs:
    """How snapshot `*_id` names map to collections; see diagram.toml."""

    aliases: dict[str, str]
    nested_types: dict[str, str]
    tables: set[str]


@dataclass(slots=True)
class _ConstraintFk:
    cols: list[str]
    target: str
    target_cols: list[str] | None
    nullable: bool
    label: str


@dataclass(slots=True)
class _Context:
    layer: LayerName
    table: str
    tables: set[str]
    diag: Diagnostics
    snapshot_refs: SnapshotRefs | None = None
    unresolved: set[str] = field(default_factory=set[str])

    @property
    def where(self) -> str:
        return f"{self.layer}.{self.table}"


def split_top(s: str) -> list[str]:
    """Split on commas that are not inside brackets, braces or parentheses."""
    out: list[str] = []
    depth = 0
    cur: list[str] = []
    for ch in s:
        if ch in "[{(":
            depth += 1
        elif ch in "]})":
            depth -= 1
        if ch == "," and depth == 0:
            out.append("".join(cur))
            cur = []
        else:
            cur.append(ch)
    out.append("".join(cur))
    return [x.strip() for x in out if x.strip()]


def _set_type(col: Column, text: str) -> None:
    t = text.strip()
    if t.endswith("?"):
        col.nullable = True
        t = t[:-1].strip()
    col.type = t
    if t.startswith(("[", "{")):
        return
    if "|" in t:
        col.enum_values = t.split("|")
    elif _ENUM_SLASH.fullmatch(t):
        col.enum_values = t.split("/")


def _strip_flags(part: str) -> tuple[str, list[str]]:
    flags: list[str] = []
    p = part
    while m := _TAIL_FLAG.search(p):
        flags.append(m.group(1))
        p = p[: m.start()].strip()
    return p, flags


def _build_fk(ctx: _Context, m: re.Match[str], pending: list[Column]) -> Column:
    c = Column(m.group(1), nullable=bool(m.group(3)))
    target = m.group(2)
    group = [*pending, c]
    for g in group:
        g.fk_target = target
        g.fk_via = "decl"
        g.nullable = g.nullable or c.nullable
        g.type = "→ " + target
        if len(group) > 1:
            g.fk_cols = [x.name for x in group]
    if target not in ctx.tables:
        ctx.diag.fk_missing.append(f"{ctx.where}.{c.name} → {target}")
    return c


def _parse_build_decl(ctx: _Context, decl: str) -> list[Column]:
    """Parse a build declaration such as `id:ID PK,card_id→card,printing_id,face_id→printing_face`."""
    cols: list[Column] = []
    pending: list[Column] = []  # bare names that lead into a composite FK like a,b→t
    for part in split_top(decl):
        p, flags = _strip_flags(part)
        if m := _BUILD_FK.fullmatch(p):
            c = _build_fk(ctx, m, pending)
            pending = []
        elif m := _TYPED.fullmatch(p):
            if pending:
                ctx.diag.errors.append(
                    f"{ctx.where}: 無型別欄位 {[x.name for x in pending]} 後面不是 FK"
                )
                pending = []
            c = Column(m.group(1))
            _set_type(c, m.group(2))
        elif re.fullmatch(NAME, p):
            c = Column(p)
            pending.append(c)
        else:
            cols.append(Column(part, raw=part))
            ctx.diag.errors.append(f"{ctx.where}: 無法解析欄位 `{part}`")
            continue
        c.pk = "PK" in flags
        c.uq.extend("UNIQUE" for f in flags if f == "UNIQUE")
        cols.append(c)
    for c in pending:
        ctx.diag.errors.append(f"{ctx.where}: 欄位 `{c.name}` 沒有型別")
    return cols


def _is_id_name(name: str) -> bool:
    return name != "id" and (name.endswith(("_id", "_ids")))


def _infer_target(ctx: _Context, name: str) -> str | None:
    """Collection a snapshot `*_id` name points at, or None when it is declared not to be a reference."""
    refs = ctx.snapshot_refs
    assert refs is not None
    for key in (f"{ctx.table}.{name}", name):
        if key in refs.aliases:
            return refs.aliases[key] or None
    base = name.removesuffix("s") if name.endswith("_ids") else name
    if base.endswith("_unit_id"):
        return "text_unit"
    words = base.removesuffix("_id").split("_")
    for i in range(len(words)):
        candidate = "_".join(words[i:])
        if candidate in refs.tables:
            return candidate
    if name not in ctx.unresolved:
        ctx.unresolved.add(name)
        ctx.diag.errors.append(
            f"{ctx.where}: 無法由欄名 `{name}` 推斷引用的集合（在 diagram.toml 的 snapshot.references 登記）"
        )
    return None


def _field_target(ctx: _Context, name: str, type_text: str) -> str | None:
    refs = ctx.snapshot_refs
    assert refs is not None
    if (qualified := f"{ctx.table}.{name}") in refs.aliases:
        return refs.aliases[qualified] or None
    if _is_id_name(name):
        return _infer_target(ctx, name)
    am = _ARRAY_OF_NAME.fullmatch(type_text)
    if am and _is_id_name(am.group(1)):
        return _infer_target(ctx, am.group(1))
    return None


def _nested_refs(ctx: _Context, type_text: str, seen: frozenset[str]) -> list[str]:
    """References to collections inside inline objects and named nested types."""
    refs = ctx.snapshot_refs
    assert refs is not None
    t = type_text.strip().removesuffix("?").strip()
    if t.startswith("[") and t.endswith("]"):
        t = t[1:-1].strip()
    if t in refs.nested_types and t not in seen:
        return _fields_refs(ctx, refs.nested_types[t], seen | {t})
    if t.startswith("{") and t.endswith("}"):
        return _fields_refs(ctx, t[1:-1], seen)
    return []


def _fields_refs(ctx: _Context, fields: str, seen: frozenset[str]) -> list[str]:
    refs = ctx.snapshot_refs
    assert refs is not None
    out: list[str] = []
    for part in split_top(fields):
        m = _SNAPSHOT_FIELD.fullmatch(part)
        if not m:
            ctx.diag.errors.append(f"{ctx.where}: 無法解析內嵌欄位 `{part}`")
            continue
        name, type_text = m.group(1), (m.group(3) or "").strip()
        if target := _field_target(ctx, name, type_text):
            out.append(target)
        # A field named after a nested type, such as `program_ref?`, carries that type.
        out.extend(_nested_refs(ctx, type_text or name, seen))
    return out


def _parse_snapshot_decl(ctx: _Context, decl: str) -> list[Column]:
    """Parse a snapshot field list; references are inferred from `*_id` names."""
    cols: list[Column] = []
    for part in split_top(decl):
        m = _SNAPSHOT_FIELD.fullmatch(part)
        if not m:
            cols.append(Column(part, raw=part))
            ctx.diag.errors.append(f"{ctx.where}: 無法解析欄位 `{part}`")
            continue
        c = Column(m.group(1), nullable=m.group(2) is not None)
        if m.group(3):
            _set_type(c, m.group(3))
            c.nullable = c.nullable or m.group(2) is not None
        if target := _field_target(ctx, c.name, c.type):
            c.fk_target = target
            c.fk_via = "inferred"
            if target not in ctx.tables:
                ctx.diag.fk_missing.append(f"{ctx.where}.{c.name} → {target}（推斷）")
        nested = dict.fromkeys(_nested_refs(ctx, c.type, frozenset()))
        c.nested_refs = [n for n in nested if n != c.fk_target]
        cols.append(c)
    return cols


def _resolve_cols(spec: str, cols: list[Column]) -> list[str]:
    spec = spec.strip()
    if spec == "全部欄":
        return [c.name for c in cols]
    if m := _COLUMN_COUNT.fullmatch(spec):
        return [c.name for c in cols[: _CN_NUM[m.group(1)]]]
    return [x.strip() for x in spec.split(",")]


def _apply_pk(ctx: _Context, cols: list[Column], cons: str) -> None:
    by = {c.name: c for c in cols}
    for m in _PK.finditer(cons):
        if m.group(1) is None:
            if not any(c.pk for c in cols):
                ctx.diag.errors.append(f"{ctx.where}: 約束提到 PK 但沒列欄位，無法標記")
            continue
        for n in _resolve_cols(m.group(1), cols):
            if n in by:
                by[n].pk = True
            else:
                ctx.diag.errors.append(f"{ctx.where}: PK 欄位 `{n}` 不在宣告中")


def _apply_uq(ctx: _Context, cols: list[Column], cons: str) -> None:
    by = {c.name: c for c in cols}
    for i, m in enumerate(_UQ.finditer(cons), 1):
        label = f"UQ{i}" + ("*" if m.group(1) else "")
        for n in (x.strip() for x in m.group(2).split(",")):
            if n in by:
                by[n].uq.append(label)
            else:
                ctx.diag.errors.append(f"{ctx.where}: UQ 欄位 `{n}` 不在宣告中")


def _apply_constraint_fks(
    ctx: _Context, cols: list[Column], cons: str
) -> list[_ConstraintFk]:
    by = {c.name: c for c in cols}
    extra: list[_ConstraintFk] = []
    seen: set[tuple[tuple[str, ...], str, tuple[str, ...] | None]] = set()
    for m in _CONSTRAINT_FK.finditer(cons):
        src = [" ".join(x.split()) for x in m.group(1).split(",")]
        target = m.group(2)
        target_cols = [x.strip() for x in m.group(3).split(",")] if m.group(3) else None
        label = f"FK({','.join(src)})→{target}" + (
            f"({','.join(target_cols)})" if target_cols else ""
        )
        if target not in ctx.tables:
            ctx.diag.fk_missing.append(f"{ctx.where} 約束 {label}")
        present = [n for n in src if n in by]
        if absent := [n for n in src if n not in by]:
            ctx.diag.notes.append(
                f"{ctx.where}: 約束 {label} 的 {absent} 不是宣告欄（常數或隱含欄）"
            )
        if not present:
            ctx.diag.errors.append(f"{ctx.where}: 約束 {label} 沒有任何宣告欄")
            continue
        # Only an identical constraint is a duplicate; a composite FK is not implied by single-column ones.
        key = (tuple(src), target, tuple(target_cols) if target_cols else None)
        if key in seen:
            ctx.diag.notes.append(f"{ctx.where}: 約束 {label} 重複出現，只畫一次")
            continue
        seen.add(key)
        for n in present:
            c = by[n]
            c.constraint_fks.append(label)
            if c.fk_target is None:
                c.fk_target = target
                c.fk_via = "constraint"
                c.fk_cols = present if len(present) > 1 else None
        extra.append(
            _ConstraintFk(
                present,
                target,
                target_cols,
                all(by[n].nullable for n in present),
                label,
            )
        )
    return extra


def _column_relations(table: str, cols: list[Column]) -> list[Relation]:
    rels: list[Relation] = []
    seen: set[tuple[str, ...]] = set()
    for c in cols:
        if not c.fk_target or c.fk_via in {None, "constraint"}:
            continue
        key = tuple(c.fk_cols or [c.name])
        if key in seen:
            continue
        seen.add(key)
        group = [x for x in cols if x.name in key]
        via: FkVia = c.fk_via or "decl"
        rels.append(
            Relation(
                table, list(key), c.fk_target, None, all(g.nullable for g in group), via
            )
        )
    for c in cols:
        rels.extend(
            Relation(table, [c.name], n, None, c.nullable, "inferred")
            for n in c.nested_refs
        )
    return rels


def _mark_snapshot_default_pk(ctx: _Context, cols: list[Column]) -> None:
    if any(c.pk for c in cols):
        return
    if cols and cols[0].name == "id":
        cols[0].pk = True
    else:
        ctx.diag.errors.append(f"{ctx.where}: 沒有寫 PK，首欄也不是 `id`，無法標記 PK")


def build_layer(
    layer: LayerName,
    raw: list[RawTable],
    diag: Diagnostics,
    snapshot_refs: SnapshotRefs | None = None,
) -> tuple[dict[str, Table], list[Relation]]:
    """Parse every table of one layer and collect the relations between them."""
    tables = {t.name for t in raw}
    out: dict[str, Table] = {}
    rels: list[Relation] = []
    for t in raw:
        ctx = _Context(layer, t.name, tables, diag, snapshot_refs)
        cols = (
            _parse_build_decl(ctx, t.declaration)
            if layer == "build"
            else _parse_snapshot_decl(ctx, t.declaration)
        )
        cons = t.constraints.replace("`", "")
        _apply_pk(ctx, cols, cons)
        _apply_uq(ctx, cols, cons)
        extra = _apply_constraint_fks(ctx, cols, cons)
        if layer == "snapshot":
            _mark_snapshot_default_pk(ctx, cols)
        elif not any(c.pk for c in cols):
            diag.errors.append(f"{ctx.where}: 找不到 PK")
        # A constraint over the same columns and table as a declared FK carries the target
        # columns too, so it replaces that edge instead of doubling it.
        covered = {(tuple(e.cols), e.target) for e in extra}
        rels.extend(
            r
            for r in _column_relations(t.name, cols)
            if r.via != "decl" or (tuple(r.cols), r.target) not in covered
        )
        rels.extend(
            Relation(
                t.name,
                e.cols,
                e.target,
                e.target_cols,
                e.nullable,
                "constraint",
                e.label,
            )
            for e in extra
        )
        fk_targets = {c.fk_target for c in cols if c.fk_target} | {
            e.target for e in extra
        }
        prose_refs = set(_ARROW_REF.findall(t.declaration + " " + cons)) & tables
        nested = sorted(prose_refs - fk_targets)
        if nested:
            diag.notes.append(
                f"{ctx.where}: 說明文字提到 →{nested}，但沒有對應的 FK 欄位"
            )
        out[t.name] = Table(t.name, cols, t.constraints, t.line, t.source, nested)
    return out, [r for r in rels if r.target in tables]
