# /// script
# requires-python = ">=3.14"
# ///
"""Build the interactive schema ER page from the layered docs/schema/ documents and open it in a browser."""

import argparse
import functools
import http.server
import json
import os
import sys
import webbrowser
from dataclasses import asdict, dataclass
from pathlib import Path

from er_config import DiagramConfig, check_aliases, check_groups, load_config
from er_markdown import read_model, source_hashes
from er_model import LAYERS, Diagnostics, LayerName, Relation, SchemaModel, Table
from er_parse import SnapshotRefs, build_layer

HERE = Path(__file__).resolve().parent
SCHEMA_DIR = HERE.parents[1] / "docs" / "schema"
TEMPLATE = HERE / "template.html"
CONFIG = HERE / "diagram.toml"
DEFAULT_OUT = HERE / "out" / "schema-er.html"
DATA_SLOT = "/*__DATA__*/null"


@dataclass(slots=True)
class LayerResult:
    """Parsed tables and relations of one layer."""

    tables: dict[str, Table]
    rels: list[Relation]


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    """Parse command-line options."""
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument(
        "--out",
        type=Path,
        default=DEFAULT_OUT,
        help="output HTML path (default: %(default)s)",
    )
    ap.add_argument(
        "--no-open", action="store_true", help="do not open the result in a browser"
    )
    ap.add_argument(
        "--serve",
        nargs="?",
        const=8000,
        type=int,
        metavar="PORT",
        help="serve the output directory on localhost (default port 8000) until Ctrl-C",
    )
    ap.add_argument(
        "--artifact",
        action="store_true",
        help="omit the document shell; claude.ai artifacts add their own doctype and charset",
    )
    ap.add_argument(
        "--model",
        type=Path,
        help="debug: read a schema-model.json written by an earlier run instead of the Markdown",
    )
    return ap.parse_args(argv)


def load_model(path: Path | None, diag: Diagnostics) -> SchemaModel:
    """Read the Markdown, or a saved model whose source hashes still match the Markdown."""
    if path is None:
        return read_model(SCHEMA_DIR, diag)
    model = SchemaModel.load(path)
    if model.sources != source_hashes(SCHEMA_DIR):
        diag.errors.append(
            f"{path} 的來源雜湊與目前的 schema 文件不符，請重新從 Markdown 產生"
        )
    return model


def parse_layers(
    model: SchemaModel, config: DiagramConfig, diag: Diagnostics
) -> dict[LayerName, LayerResult]:
    """Parse both layers and check the groups against them."""
    snapshot_tables = {t.name for t in model.snapshot}
    check_aliases(config.snapshot_aliases, snapshot_tables, diag)
    refs = SnapshotRefs(
        aliases=config.snapshot_aliases,
        nested_types={t.name: t.declaration for t in model.nested_types},
        tables=snapshot_tables,
    )
    out: dict[LayerName, LayerResult] = {}
    for layer in LAYERS:
        tables, rels = build_layer(
            layer, model.layer(layer), diag, refs if layer == "snapshot" else None
        )
        check_groups(layer, config.groups[layer], set(tables), diag)
        out[layer] = LayerResult(tables, rels)
    return out


def check_constraint_fks(
    model: SchemaModel,
    config: DiagramConfig,
    layers: dict[LayerName, LayerResult],
    diag: Diagnostics,
) -> None:
    """Every `FK(` in the documents must become a constraint relation."""
    written = sum(
        t.constraints.count("FK(") for layer in LAYERS for t in model.layer(layer)
    )
    labels = {
        f"{r.source}: {r.label}"
        for lr in layers.values()
        for r in lr.rels
        if r.via == "constraint"
    }
    drawn = sum(r.via == "constraint" for lr in layers.values() for r in lr.rels)
    if drawn != written:
        diag.errors.append(
            f"文件寫了 {written} 個 FK(...)，圖上只有 {drawn} 條約束關係"
        )
    diag.errors.extend(
        f"diagram.toml checks.constraint_fks：找不到約束關係 {x}"
        for x in config.required_constraints
        if x not in labels
    )
    anchors = {
        f"{r.source}.{r.cols[0]} → {r.target}.{r.target_anchor}"
        for lr in layers.values()
        for r in lr.rels
        if r.via == "constraint"
    }
    diag.errors.extend(
        f"diagram.toml checks.constraint_anchors：圖線錨點不是 {x}"
        for x in config.required_anchors
        if x not in anchors
    )


def fill_template(template: str, payload: str) -> str:
    """Put the data into the template's single data slot."""
    if (n := template.count(DATA_SLOT)) != 1:
        msg = f"template.html 的資料插槽 {DATA_SLOT} 應恰好出現一次，實際 {n} 次"
        raise ValueError(msg)
    return template.replace(DATA_SLOT, payload)


def render_html(
    model: SchemaModel,
    config: DiagramConfig,
    layers: dict[LayerName, LayerResult],
    *,
    artifact: bool,
) -> str:
    """Fill template.html with the parsed data."""
    data = {
        "layers": {
            name: {
                "tables": {k: asdict(t) for k, t in lr.tables.items()},
                "rels": [r.to_json() for r in lr.rels],
                "groups": [asdict(g) for g in config.groups[name]],
            }
            for name, lr in layers.items()
        },
        "sources": model.sources,
    }
    payload = json.dumps(data, ensure_ascii=False, separators=(",", ":")).replace(
        "</", "<\\/"
    )
    html = fill_template(TEMPLATE.read_text(encoding="utf-8"), payload)
    if artifact:
        return html
    # Opened from disk, the page needs the charset and viewport the artifact host would supply.
    return (
        '<!doctype html>\n<html lang="zh-Hant">\n<meta charset="utf-8">\n'
        '<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">\n'
        f"{html}\n</html>\n"
    )


def report_lines(
    model: SchemaModel, layers: dict[LayerName, LayerResult], diag: Diagnostics
) -> list[str]:
    """Human-readable parse report."""
    n_cols = sum(len(t.cols) for lr in layers.values() for t in lr.tables.values())
    n_rels = {k: len(v.rels) for k, v in layers.items()}
    n_constraint = sum(r.via == "constraint" for lr in layers.values() for r in lr.rels)
    snapshot_pk = sum(
        any(c.pk for c in t.cols) for t in layers["snapshot"].tables.values()
    )
    return [
        "schema-er 解析報告",
        *(f"來源：{name} sha256={digest}" for name, digest in model.sources.items()),
        (
            f"表數：建置 {len(layers['build'].tables)}、快照 {len(layers['snapshot'].tables)}"
            f"（其中 {snapshot_pk} 個標到 PK）；欄位 {n_cols}；關係 {n_rels}"
            f"（約束 FK {n_constraint}）"
        ),
        "",
        f"## 無法解析／無法標記（{len(diag.errors)}）",
        *(f"- {x}" for x in diag.errors),
        "",
        f"## FK 目標不存在（{len(diag.fk_missing)}）",
        *(f"- {x}" for x in diag.fk_missing),
        "",
        f"## 備註（{len(diag.notes)}）",
        *(f"- {x}" for x in diag.notes),
    ]


def main(argv: list[str] | None = None) -> int:
    """Generate the page; return a non-zero status when anything could not be parsed."""
    args = parse_args(argv)
    out: Path = args.out.resolve()
    diag = Diagnostics()
    model = load_model(args.model, diag)
    config = load_config(CONFIG)
    layers = parse_layers(model, config, diag)
    check_constraint_fks(model, config, layers, diag)
    html = ""
    try:
        html = render_html(model, config, layers, artifact=args.artifact)
    except ValueError as e:
        diag.errors.append(str(e))

    out.parent.mkdir(parents=True, exist_ok=True)
    if args.model is None:
        model.dump(out.with_name("schema-model.json"))
    lines = report_lines(model, layers, diag)
    report = out.with_name("parse-report.txt")
    report.write_text("\n".join(lines) + "\n", encoding="utf-8")

    if diag.failed:
        out.unlink(missing_ok=True)
        sys.stderr.write(
            "\n".join(lines) + f"\n\n解析失敗，未產生 HTML；報告：{report}\n"
        )
        return 1
    out.write_text(html, encoding="utf-8")
    summary = next(x for x in lines if x.startswith("表數"))
    sys.stdout.write(
        f"{summary}\n備註 {len(diag.notes)} 則（見 {report}）\nHTML：{out}\n"
    )
    url = f"http://localhost:{args.serve}/{out.name}" if args.serve else out.as_uri()
    if args.no_open:
        pass
    elif is_headless_remote():
        # Over SSH without a display, webbrowser would pick a text browser on the remote host.
        sys.stdout.write(remote_hint(out, args.serve))
    else:
        webbrowser.open(url)
    if args.serve:
        serve(out.parent, args.serve)
    return 0


def is_headless_remote() -> bool:
    """Tell whether this runs in an SSH session with no graphical display."""
    ssh = "SSH_CONNECTION" in os.environ or "SSH_TTY" in os.environ
    display = "DISPLAY" in os.environ or "WAYLAND_DISPLAY" in os.environ
    return ssh and not display


def remote_hint(out: Path, port: int | None) -> str:
    """Explain how to view the page from the local machine."""
    p = port or 8000
    serve_line = "" if port else f"  uv run tools/schema-er/build_er.py --serve {p}\n"
    return (
        "偵測到 SSH 且沒有圖形環境，不自動開啟瀏覽器。在本機瀏覽器查看：\n"
        f"{serve_line}"
        f"  VS Code Remote-SSH 會自動轉送 port；一般 SSH 請用 ssh -L {p}:localhost:{p} <主機>\n"
        f"  然後開 http://localhost:{p}/{out.name}\n"
    )


def serve(directory: Path, port: int) -> None:
    """Serve the output directory on localhost until interrupted."""
    handler = functools.partial(
        http.server.SimpleHTTPRequestHandler, directory=str(directory)
    )
    with http.server.ThreadingHTTPServer(("127.0.0.1", port), handler) as httpd:
        sys.stdout.write(
            f"serving {directory} at http://localhost:{port}/ （Ctrl-C 結束）\n"
        )
        sys.stdout.flush()
        try:
            httpd.serve_forever()
        except KeyboardInterrupt:
            sys.stdout.write("\n")


if __name__ == "__main__":
    sys.exit(main())
