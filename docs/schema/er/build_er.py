# /// script
# requires-python = ">=3.12"
# ///
"""Build the interactive schema ER page from schema-model.json and open it in a browser."""

import argparse
import json
import re
import webbrowser
from pathlib import Path

HERE = Path(__file__).resolve().parent
TEMPLATE = HERE / "template.html"
DEFAULT_OUT = HERE / "out" / "schema-er.html"

CN_NUM = {"兩": 2, "二": 2, "三": 3, "四": 4, "五": 5}

issues: list[str] = []
fk_missing: list[str] = []
notes: list[str] = []


def split_top(s: str) -> list[str]:
    """Split on commas that are not inside [] {} ()."""
    out, depth, cur = [], 0, []
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


def new_col(name: str) -> dict:
    return {"name": name, "type": "", "nullable": False, "pk": False, "fk_target": None,
            "fk_cols": None, "fk_via": None, "fk_inferred": False, "uq": [], "enum_values": None,
            "raw": None}


NAME = r"[A-Za-z_][A-Za-z0-9_]*"


def parse_type(col: dict, t: str) -> None:
    t = t.strip()
    if t.endswith("?"):
        col["nullable"] = True
        t = t[:-1].strip()
    col["type"] = t
    if not t.startswith("[") and not t.startswith("{"):
        if "|" in t:
            col["enum_values"] = t.split("|")
        elif "/" in t and re.fullmatch(r"[a-z_]+(/[a-z_]+)+", t):
            col["enum_values"] = t.split("/")


def parse_build_decl(tname: str, decl: str, tables: set[str]) -> list[dict]:
    cols: list[dict] = []
    pending_bare: list[dict] = []  # bare names that precede a composite FK like a,b→t
    for part in split_top(decl):
        tail_flags = []
        p = part
        while True:
            m = re.search(r"\s+(PK|UNIQUE)$", p)
            if not m:
                break
            tail_flags.append(m.group(1))
            p = p[: m.start()].strip()
        m_fk = re.fullmatch(rf"({NAME})\s*→\s*({NAME})(\?)?", p)
        m_ty = re.fullmatch(rf"({NAME})\s*:\s*(.+)", p)
        m_bare = re.fullmatch(NAME, p)
        if m_fk:
            c = new_col(m_fk.group(1))
            target = m_fk.group(2)
            c["nullable"] = bool(m_fk.group(3))
            c["type"] = "→ " + target
            group = pending_bare + [c]
            for g in group:
                g["fk_target"] = target
                g["fk_via"] = "decl"
                g["nullable"] = g["nullable"] or c["nullable"]
                if len(group) > 1:
                    g["fk_cols"] = [x["name"] for x in group]
                    g["type"] = "→ " + target
            if target not in tables:
                fk_missing.append(f"build.{tname}.{c['name']} → {target}")
            pending_bare = []
            cols.append(c)
        elif m_ty:
            if pending_bare:
                issues.append(f"build.{tname}: 無型別欄位 {[x['name'] for x in pending_bare]} 後面不是 FK")
                pending_bare = []
            c = new_col(m_ty.group(1))
            parse_type(c, m_ty.group(2))
            cols.append(c)
        elif m_bare:
            c = new_col(p)
            cols.append(c)
            pending_bare.append(c)
        else:
            c = new_col(part)
            c["raw"] = part
            issues.append(f"build.{tname}: 無法解析欄位 `{part}`")
            cols.append(c)
            continue
        for f in tail_flags:
            if f == "PK":
                c["pk"] = True
            else:
                c["uq"].append("UNIQUE")
    for c in pending_bare:
        issues.append(f"build.{tname}: 欄位 `{c['name']}` 沒有型別")
    return cols


SNAP_ALIAS = {
    "home_set_id": "product_family", "family_id": "product_family",
    "old_card_id": "card", "new_card_id": "card", "from_card_id": "card", "to_card_id": "card",
    "interaction_target_id": "digital_card", "profile_id": "rules_profile",
    "image_id": "image_asset", "source_unit_id": "text_unit", "ruling_id": None,
    "replacement_revision_id": "ruling_revision", "revision_id": "face_revision",
    "scope_unit_id": "text_unit",
}


def infer_snapshot_target(tname: str, name: str, tables: set[str]) -> str | None:
    if tname == "qa" and name == "current_version_id":
        return "qa_version"
    if name in SNAP_ALIAS:
        return SNAP_ALIAS[name]
    if name.endswith("_unit_id"):
        return "text_unit"
    if name.endswith("_id"):
        base = name[:-3]
        if base in tables:
            return base
    return None


def parse_snapshot_decl(tname: str, decl: str, tables: set[str], refs: list[str]) -> list[dict]:
    cols: list[dict] = []
    for part in split_top(decl):
        m = re.fullmatch(rf"({NAME})(\?)?(?:\s*:\s*(.+))?", part, re.S)
        if not m:
            c = new_col(part)
            c["raw"] = part
            issues.append(f"snapshot.{tname}: 無法解析欄位 `{part}`")
            cols.append(c)
            continue
        c = new_col(m.group(1))
        c["nullable"] = bool(m.group(2))
        if m.group(3):
            parse_type(c, m.group(3))
            c["nullable"] = c["nullable"] or m.group(2) is not None
        target = infer_snapshot_target(tname, c["name"], tables)
        # Array of IDs such as faces:[face_id] / cards:[card_id]
        am = re.fullmatch(rf"\[({NAME})\]", c["type"])
        if not target and am:
            target = infer_snapshot_target(tname, am.group(1), tables)
        if target:
            c["fk_target"] = target
            c["fk_via"] = "inferred"
            c["fk_inferred"] = True
            if target not in tables:
                fk_missing.append(f"snapshot.{tname}.{c['name']} → {target}（推斷）")
            elif target not in refs:
                notes.append(f"snapshot.{tname}.{c['name']} → {target}：由欄位名推斷，但不在 refs 內")
        cols.append(c)
    # snapshot has no PK markers; a leading `id` column is the entity id
    if cols and cols[0]["name"] == "id":
        cols[0]["pk"] = True
    return cols


def apply_constraints(layer: str, tname: str, cols: list[dict], cons: str, tables: set[str]) -> list[dict]:
    """Mark PK/UQ from constraint text; return extra constraint-level FK groups."""
    by = {c["name"]: c for c in cols}
    extra_fks: list[dict] = []

    def resolve(spec: str) -> list[str] | None:
        spec = spec.strip()
        if spec == "全部欄":
            return [c["name"] for c in cols]
        m = re.fullmatch(r"前?([兩二三四五])欄", spec)
        if m:
            return [c["name"] for c in cols[: CN_NUM[m.group(1)]]]
        names = [x.strip() for x in spec.split(",")]
        return names

    for m in re.finditer(r"(?<![A-Za-z])PK(?:\(([^)]*)\))?", cons):
        if m.group(1) is None:
            if any(c["pk"] for c in cols):
                continue
            issues.append(f"{layer}.{tname}: 約束寫「複合 PK」但沒列欄位，未標到欄位上")
            continue
        names = resolve(m.group(1)) or []
        for n in names:
            if n in by:
                by[n]["pk"] = True
            else:
                issues.append(f"{layer}.{tname}: PK 欄位 `{n}` 不在宣告中")
    uq_i = 0
    for m in re.finditer(r"(部分\s*)?UQ\(([^)]*)\)", cons):
        uq_i += 1
        label = f"UQ{uq_i}" + ("*" if m.group(1) else "")
        for n in [x.strip() for x in m.group(2).split(",")]:
            if n in by:
                by[n]["uq"].append(label)
            else:
                issues.append(f"{layer}.{tname}: UQ 欄位 `{n}` 不在宣告中")
    for m in re.finditer(rf"FK\(([^)]*)\)→({NAME})(?:\(([^)]*)\))?", cons):
        src_cols = [x.strip() for x in m.group(1).split(",")]
        target = m.group(2)
        tcols = [x.strip() for x in m.group(3).split(",")] if m.group(3) else None
        if target not in tables:
            fk_missing.append(f"{layer}.{tname} 約束 FK({m.group(1)}) → {target}")
        present = [n for n in src_cols if n in by]
        absent = [n for n in src_cols if n not in by]
        if absent:
            notes.append(f"{layer}.{tname}: 約束 FK({m.group(1)})→{target} 的欄位 {absent} 不在宣告中（常數或隱含欄）")
        if not present:
            continue
        # redundant if some column already declares an FK to the same target
        if any(by[n]["fk_target"] == target for n in present):
            continue
        for n in present:
            c = by[n]
            if c["fk_target"] is None:
                c["fk_target"] = target
                c["fk_via"] = "constraint"
                c["fk_cols"] = present if len(present) > 1 else None
        extra_fks.append({"cols": present, "target": target, "target_cols": tcols,
                          "nullable": all(by[n]["nullable"] for n in present)})
    return extra_fks


def build_layer(layer: str, model: dict, tables: set[str]) -> tuple[dict, list[dict]]:
    out: dict = {}
    rels: list[dict] = []
    for tname, t in model.items():
        if layer == "build":
            cols = parse_build_decl(tname, t["declaration"], tables)
        else:
            cols = parse_snapshot_decl(tname, t["declaration"], tables, t["refs"])
        cons = t["constraints"].strip().lstrip("；").strip().rstrip("|").strip()
        extra = apply_constraints(layer, tname, cols, cons, tables)
        seen_groups = set()
        for c in cols:
            if not c["fk_target"] or c["fk_via"] == "constraint":
                continue
            key = tuple(c["fk_cols"] or [c["name"]])
            if key in seen_groups:
                continue
            seen_groups.add(key)
            group = [x for x in cols if x["name"] in key]
            rels.append({"from": tname, "col": key[0], "cols": list(key), "to": c["fk_target"],
                         "to_col": None, "nullable": all(g["nullable"] for g in group),
                         "via": c["fk_via"]})
        for e in extra:
            rels.append({"from": tname, "col": e["cols"][0], "cols": e["cols"], "to": e["target"],
                         "to_col": (e["target_cols"] or [None])[0], "nullable": e["nullable"],
                         "via": "constraint"})
        fk_targets = {c["fk_target"] for c in cols if c["fk_target"]} | {e["target"] for e in extra}
        nested = [r for r in t["refs"] if r not in fk_targets]
        if layer == "build" and nested:
            notes.append(f"build.{tname}: refs 有 {nested} 但沒有對應 FK 欄位")
        out[tname] = {
            "name": tname, "cols": cols, "constraints": cons, "line": t["line"],
            "source": t["source"], "refs": t["refs"], "nested_refs": nested,
        }
    rels = [r for r in rels if r["to"] in tables]
    return out, rels


def parse_args() -> argparse.Namespace:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--model", type=Path, required=True, help="schema-model.json produced from the schema docs")
    ap.add_argument("--out", type=Path, default=DEFAULT_OUT, help="output HTML path")
    ap.add_argument("--no-open", action="store_true", help="do not open the result in a browser")
    ap.add_argument("--artifact", action="store_true",
                    help="omit the document shell; claude.ai artifacts add their own doctype and charset")
    return ap.parse_args()


def main() -> None:
    args = parse_args()
    src: Path = args.model
    out: Path = args.out
    report = out.with_name("parse-report.txt")
    d = json.loads(src.read_text(encoding="utf-8"))
    data: dict = {"layers": {}}
    for layer in ("build", "snapshot"):
        tables = set(d[layer])
        tdata, rels = build_layer(layer, d[layer], tables)
        groups = []
        grouped: set[str] = set()
        for key, title, names in d["groups"][layer]:
            members = names.split()
            for n in members:
                if n not in tables:
                    issues.append(f"{layer} 分組 {key} 含不存在的表 {n}")
            members = [n for n in members if n in tables]
            grouped |= set(members)
            short = re.sub(r"^(建置|快照)：", "", title)
            groups.append({"key": key, "title": title, "short": short, "tables": members})
        ungrouped = sorted(tables - grouped)
        if ungrouped:
            notes.append(f"{layer} 未分組的表：{ungrouped}")
        data["layers"][layer] = {"tables": tdata, "rels": rels, "groups": groups}
    data["sources"] = d["sources"]

    html = TEMPLATE.read_text(encoding="utf-8")
    payload = json.dumps(data, ensure_ascii=False, separators=(",", ":")).replace("</", "<\\/")
    html = html.replace("/*__DATA__*/null", payload)
    if not args.artifact:
        # Opened from disk, the page needs the charset and viewport the artifact host would supply.
        html = ('<!doctype html>\n<html lang="zh-Hant">\n<meta charset="utf-8">\n'
                '<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">\n'
                + html + "\n</html>\n")
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(html, encoding="utf-8")

    n_cols = sum(len(t["cols"]) for L in data["layers"].values() for t in L["tables"].values())
    n_rels = {k: len(v["rels"]) for k, v in data["layers"].items()}
    lines = [
        "schema-er 解析報告",
        f"來源：{src}",
        f"表數：build {len(d['build'])}、snapshot {len(d['snapshot'])}；欄位總數 {n_cols}；關係數 {n_rels}",
        "",
        f"## 無法解析／無法標記（{len(issues)}）",
        *[f"- {x}" for x in issues],
        "",
        f"## FK 目標不存在（{len(fk_missing)}）",
        *[f"- {x}" for x in fk_missing],
        "",
        f"## 備註（{len(notes)}）",
        *[f"- {x}" for x in notes],
    ]
    report.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print("\n".join(lines[:3]))
    print(f"issues={len(issues)} fk_missing={len(fk_missing)} notes={len(notes)} html={out}")
    if not args.no_open:
        webbrowser.open(out.resolve().as_uri())


if __name__ == "__main__":
    main()
