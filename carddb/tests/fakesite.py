"""A fake Japanese official site, shaped like the real pages."""

from urllib.parse import parse_qs, urlsplit

import httpx

from sve_carddb.sources import official_jp as jp

PADDING = "<!--" + "x" * 1200 + "-->"
IMG = "/wordpress/wp-content/images/cardlist"


class FakeSite:
    """Serves pages shaped like the real site, from an editable product catalogue."""

    def __init__(self, sets: dict[str, int]) -> None:
        self.sets = sets
        self.calls: list[str] = []
        self.card_number_override: dict[str, str] = {}
        self.list_page_one_totals: list[int] = []

    def numbers(self, code: str) -> list[str]:
        return [f"{code}-{i:03d}" for i in range(1, self.sets[code] + 1)]

    def __call__(self, request: httpx.Request) -> httpx.Response:
        self.calls.append(str(request.url))
        parts = urlsplit(str(request.url))
        query = {k: v[0] for k, v in parse_qs(parts.query).items()}
        if parts.path == "/cardlist/" and "cardno" in query:
            return html(self.card_page(query["cardno"]))
        if parts.path == "/cardlist/":
            return html(self.sets_page())
        code = query["expansion_name"]
        if parts.path == "/cardlist/cardsearch/":
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
        return page(
            '<div class="cardlist-Detail">'
            f'<div class="img"><img src="{IMG}/{code}/{number.lower()}.png"></div>'
            f'<p class="ttl">Card {number}</p>'
            f'<div class="illustrator"><span class="heading">{shown}</span></div>'
            "</div>"
        )


def page(body: str) -> str:
    return f"<!DOCTYPE html><html><body>{body}{PADDING}</body></html>"


def html(body: str) -> httpx.Response:
    return httpx.Response(
        200, text=body, headers={"content-type": "text/html; charset=UTF-8"}
    )
