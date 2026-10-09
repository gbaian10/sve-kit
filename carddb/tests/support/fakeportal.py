"""A fake shadowverse-portal.com: the card API, card pages and images."""

from urllib.parse import parse_qs, urlsplit

import httpx
import orjson

from sve_carddb.parse.pages import official_sv1 as sv1

from .fakesite import PNG, html, page

ETAG = '"api-v1"'
IMAGE_ETAG = '"img-v1"'


def card_id(index: int) -> int:
    return 100_000_000 + index * 10


class FakePortal:
    """Even-numbered cards are followers (two images); the rest have one."""

    def __init__(self, count: int = sv1.MIN_CARDS) -> None:
        self.cards: list[dict[str, object]] = [
            {
                "card_id": card_id(i),
                # Some real tokens have no name.
                "card_name": f"Card {i}" if i != 1 else None,
                "char_type": sv1.FOLLOWER if i % 2 == 0 else 4,
                "skill_disc": "",
            }
            for i in range(count)
        ]
        self.calls: list[str] = []
        self.conditional: list[str | None] = []
        self.missing: set[str] = set()
        """URL paths that redirect to an HTML page, as missing images do."""
        self.gone: set[str] = set()
        """URL paths that answer 404."""
        self.page_images: dict[int, list[str]] = {}
        """Card pages whose `<img src>` differ from the template."""
        self.no_page: set[int] = set()
        """Cards whose page is the site's error page."""

    def __call__(self, request: httpx.Request) -> httpx.Response:
        self.calls.append(str(request.url))
        self.conditional.append(request.headers.get("if-none-match"))
        parts = urlsplit(str(request.url))
        if parts.path in self.gone:
            return httpx.Response(404, text="Not Found")
        if parts.path in self.missing:
            return self.missing_image(parts.path, parts.query)
        if parts.path == "/api/v1/cards":
            return self.api(request)
        if parts.path.startswith("/image/"):
            return httpx.Response(
                200,
                content=PNG,
                headers={"Content-Type": "image/png", "ETag": IMAGE_ETAG},
            )
        assert parts.path.startswith("/card/")
        assert parse_qs(parts.query)["lang"] == [sv1.CARD_PAGE_LANGUAGE]
        return html(self.card_page(int(parts.path.rsplit("/", 1)[1])))

    @staticmethod
    def missing_image(path: str, query: str) -> httpx.Response:
        if query:
            return html(page("<p>not an image</p>"))
        return httpx.Response(302, headers={"Location": f"{sv1.BASE}{path}?lang=ja"})

    def api(self, request: httpx.Request) -> httpx.Response:
        if request.headers.get("if-none-match") == ETAG:
            return httpx.Response(304, headers={"ETag": ETAG})
        body = orjson.dumps(
            {"data_headers": {}, "data": {"cards": self.cards, "errors": []}}
        )
        return httpx.Response(
            200,
            content=body,
            headers={"Content-Type": "application/json", "ETag": ETAG},
        )

    def card_page(self, number: int) -> str:
        if number in self.no_page:
            return page('<h1 class="el-heading-error">Error</h1>')
        srcs = self.page_images.get(number) or [
            f"{sv1.image_url(number, face)}?202609261156" for face in sv1.FACES
        ]
        images = "".join(
            f'<div class="card-main-image"><img src="{src}" alt="">'
            f'<div class="card-main-image-cardname"><img src="{sv1.BASE}/image/card/'
            f'phase2/ja/N/N_{number}.png?1" alt=""></div></div>'
            for src in srcs
        )
        return page(
            f'<div class="card"><h1 class="card-main-title">Card {number}</h1>'
            f'<div class="card-main-image-section">{images}</div></div>'
        )
