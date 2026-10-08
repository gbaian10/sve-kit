"""A fake shadowverse-wb.com card list API that picks the language by header."""

from urllib.parse import parse_qs, urlsplit

import httpx
import orjson

from sve_carddb.parse.pages import official_svwb as svwb

from .fakesite import PNG


class FakeWb:
    """`count` cards, served 30 per page; names carry the `Lang` header."""

    def __init__(self, count: int = svwb.MIN_CARDS + 5) -> None:
        self.ids = [10_000_000 + i for i in range(count)]
        self.calls: list[tuple[str, str | None]] = []
        self.grow_after: int | None = None
        """Add a card once this many requests were served, as a release would."""
        self.repeat = False
        """Serve the first page's cards again on every page."""

    def __call__(self, request: httpx.Request) -> httpx.Response:
        lang = request.headers.get("lang")
        self.calls.append((str(request.url), lang))
        parts = urlsplit(str(request.url))
        if parts.path.startswith("/uploads/card_image/"):
            return httpx.Response(
                200, content=PNG, headers={"Content-Type": "image/png"}
            )
        assert parts.path == "/web/CardList/cardList"
        query = parse_qs(parts.query)
        # The server ignores the query; the header must match what the URL names.
        assert query["lang"] == [lang]
        assert query["include_token"] == ["1"]
        if self.grow_after is not None and len(self.calls) > self.grow_after:
            self.ids.append(self.ids[-1] + 1)
            self.grow_after = None
        offset = 0 if self.repeat else int(query["offset"][0])
        ids = self.ids[offset : offset + svwb.PAGE_SIZE]
        body = orjson.dumps(
            {
                "data_headers": {"result_code": 1},
                "data": {
                    "count": len(self.ids),
                    "sort_card_id_list": ids,
                    "card_details": {
                        str(i): self.details(i, f"{lang} {i}") for i in ids
                    },
                },
            }
        )
        return httpx.Response(
            200, content=body, headers={"Content-Type": "application/json"}
        )

    @staticmethod
    def details(card_id: int, name: str) -> dict[str, object]:
        """Even cards are followers with an evolved image; every tenth has a style."""
        follower = card_id % 2 == 0
        styles = (
            [{"hash": image_hash(card_id, "s"), "evo_hash": ""}]
            if card_id % 10 == 0
            else []
        )
        return {
            "common": {
                "card_id": card_id,
                "name": name,
                "card_image_hash": image_hash(card_id, "c"),
            },
            "evo": {"card_image_hash": image_hash(card_id, "e")} if follower else [],
            "style_card_list": styles,
        }


def image_hash(card_id: int, kind: str) -> str:
    return f"{card_id:030x}{ord(kind):02x}"[-32:]
