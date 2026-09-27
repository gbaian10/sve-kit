"""A fake official site (Japanese or English), shaped like the real pages."""

import struct
import zlib
from urllib.parse import parse_qs, urlsplit

import httpx

from sve_carddb.sources import official_jp as jp

PADDING = "<!--" + "x" * 1200 + "-->"
IMG = "/wordpress/wp-content/images/cardlist"


class FakeSite:
    """Serves pages shaped like the real site, from an editable product catalogue."""

    def __init__(self, sets: dict[str, int], *, english: bool = False) -> None:
        self.sets = sets
        # The English site serves the same markup under other paths.
        self.english = english
        self.card_dir = "/cards/" if english else "/cardlist/"
        self.first_path = "searchresults/" if english else "cardsearch/"
        self.set_param = "expansion" if english else "expansion_name"
        self.calls: list[str] = []
        self.card_number_override: dict[str, str] = {}
        self.list_page_one_totals: list[int] = []
        self.broken_images: set[str] = set()
        # Image paths that cannot be derived from the card number.
        self.image_override: dict[str, str] = {}

    def numbers(self, code: str) -> list[str]:
        suffix = "EN" if self.english else ""
        return [f"{code}-{i:03d}{suffix}" for i in range(1, self.sets[code] + 1)]

    def __call__(self, request: httpx.Request) -> httpx.Response:
        self.calls.append(str(request.url))
        parts = urlsplit(str(request.url))
        query = {k: v[0] for k, v in parse_qs(parts.query).items()}
        if parts.path.startswith(IMG):
            if parts.path in self.broken_images:
                return httpx.Response(
                    200, content=PNG[:40], headers={"Content-Type": "image/png"}
                )
            return httpx.Response(
                200, content=PNG, headers={"Content-Type": "image/png"}
            )
        if parts.path == self.card_dir and "cardno" in query:
            return html(self.card_page(query["cardno"]))
        if parts.path == self.card_dir:
            return html(self.sets_page())
        code = query[self.set_param]
        if parts.path == self.card_dir + self.first_path:
            return html(self.list_first(code))
        return html(self.list_more(code, int(query["page"])))

    def sets_page(self) -> str:
        options = "".join(f'<option value="{c}">{c} pack</option>' for c in self.sets)
        return page(
            f'<select name="expansion_name"><option value="">指定なし</option>{options}</select>'
        )

    def list_first(self, code: str) -> str:
        total = (
            self.list_page_one_totals.pop(0)
            if self.list_page_one_totals
            else self.sets[code]
        )
        max_page = -(-total // jp.PAGE_SIZE)
        items = self.items(code, 1)
        return page(
            f'<span class="num bold">{total}</span>'
            f'<ul class="cardlist-Result_List">{items}</ul>'
            f"<script>var max_page = {max_page};</script>"
        )

    def list_more(self, code: str, page_no: int) -> str:
        return self.items(code, page_no, css="ex-item")

    def items(self, code: str, page_no: int, css: str = "") -> str:
        chunk = self.numbers(code)[
            (page_no - 1) * jp.PAGE_SIZE : page_no * jp.PAGE_SIZE
        ]
        return "".join(
            f'<li class="{css}"><p class="number">{n}</p></li>' for n in chunk
        )

    def card_page(self, number: str) -> str:
        shown = self.card_number_override.get(number, number)
        code = number.split("-", maxsplit=1)[0]
        src = self.image_override.get(number, f"{IMG}/{code}/{number.lower()}.png")
        return page(
            '<div class="cardlist-Detail"><div class="cardlist-Detail_Box_Inner">'
            f'<div class="img"><img src="{src}"></div>'
            f'<p class="ttl">Card {number}</p>'
            '<div class="info">'
            "<dl><dt>クラス</dt><dd>エルフ</dd></dl>"
            "<dl><dt>カード種類</dt><dd>フォロワー</dd></dl>"
            "<dl><dt>タイプ</dt><dd>妖精</dd></dl>"
            "<dl><dt>レアリティ</dt><dd>BR</dd></dl>"
            f"<dl><dt>収録商品</dt><dd>{code} pack</dd></dl>"
            "</div>"
            '<div class="status">'
            '<span class="status-Item status-Item-Cost"><span class="heading">コスト</span>1</span>'
            '<span class="status-Item status-Item-Power"><span class="heading">攻撃力</span>1</span>'
            '<span class="status-Item status-Item-Hp"><span class="heading">体力</span>1</span>'
            "</div>"
            f'<div class="illustrator"><span class="heading">{shown}</span></div>'
            "</div></div>"
        )


def png_chunk(kind: bytes, data: bytes) -> bytes:
    body = kind + data
    return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))


# The smallest well-formed PNG: a 1x1 grey pixel.
PNG = (
    b"\x89PNG\r\n\x1a\n"
    + png_chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 8, 0, 0, 0, 0))
    + png_chunk(b"IDAT", zlib.compress(b"\x00\x80"))
    + png_chunk(b"IEND", b"")
)


def page(body: str) -> str:
    return f"<!DOCTYPE html><html><body>{body}{PADDING}</body></html>"


def html(body: str) -> httpx.Response:
    return httpx.Response(
        200, text=body, headers={"content-type": "text/html; charset=UTF-8"}
    )
