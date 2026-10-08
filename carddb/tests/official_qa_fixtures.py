"""Invented Q&A pages and an in-process conditional HTTP server."""

from dataclasses import dataclass, field
from html import escape

import httpx

from sve_carddb.core.json import digest
from sve_carddb.parse.pages import official_en, official_jp

ROOT = f"https://{official_jp.HOST}/qa/synthetic/"
SECOND = ROOT + "?page=2"
DETAIL = ROOT + "detail/"


def block(
    number: str = "Q900000",
    answer: str = "Synthetic answer A.",
    *,
    state: str = "active",
    anchor: str = "",
    date: str = "2026/10/1",
    links: tuple[str, ...] = (),
) -> str:
    return (
        f'<div class="qa-List_Item" id="{escape(anchor)}" data-state="{state}">'
        f'<div class="qa-List_Ttl">{number} ({date})</div>'
        '<div class="qa-List_Txt-Q">Synthetic question?</div>'
        f'<div class="qa-List_Txt-A">{answer}</div><div class="qa-List_Cards">'
        + "".join(f'<a href="{escape(link)}">Synthetic card</a>' for link in links)
        + "</div></div>"
    )


def listing(items: str, *, page: int = 1, maximum: int = 2, total: int = 5) -> bytes:
    return (
        f'<html><div class="qa-List" data-page="{page}" data-max-page="{maximum}" data-total="{total}">'
        f'{items}<div class="qa-Pager"><a data-page="1" href="{ROOT}">1</a>'
        f'<a data-page="2" href="{SECOND}">2</a></div></div></html>'
    ).encode()


def detail_link() -> str:
    return f'<div class="qa-List_Item"><a class="qa-List_Link" href="{DETAIL}">Synthetic detail</a></div>'


def bodies(
    answer: str = "Synthetic answer B.", *, state: str = "active"
) -> dict[str, bytes]:
    return {
        ROOT: listing(
            block(links=(official_jp.card_url("TEST-001Ⓢa"),))
            + block("", anchor="unnumbered", date="unknown")
            + detail_link()
        ),
        SECOND: listing(
            block(answer=answer, state=state, links=(official_jp.card_url("TEST-002"),))
            + detail_link(),
            page=2,
        ),
        DETAIL: (
            '<div class="qa-List">'
            + block(
                "Q900002",
                links=(
                    official_jp.card_url("TEST-001Ⓢa"),
                    official_jp.card_url("TEST-002"),
                    official_jp.card_url("TEST-MISSING"),
                ),
            )
            + "</div>"
        ).encode(),
    }


@dataclass
class Clock:
    now: float = 0.0

    def read(self) -> float:
        return self.now

    async def sleep(self, seconds: float) -> None:
        self.now += seconds


class SyntheticInterruptedError(RuntimeError):
    pass


@dataclass
class Server:
    clock: Clock
    pages: dict[str, bytes] = field(default_factory=bodies)
    requests: list[tuple[str, float, int]] = field(default_factory=list)
    interrupt: str | None = None
    unchanged_200: bool = False
    change_on_recheck: bytes | None = None

    def respond(self, request: httpx.Request) -> httpx.Response:
        url = str(request.url)
        assert request.headers["User-Agent"].startswith("Mozilla/")
        assert request.url.host in {official_jp.HOST, official_en.HOST}
        if url == self.interrupt:
            raise SyntheticInterruptedError("Synthetic interrupted request")
        raw = self.pages.get(url)
        if raw is None:
            return httpx.Response(404)
        if (
            self.change_on_recheck is not None
            and url == ROOT
            and any(prior[0] == ROOT for prior in self.requests)
        ):
            raw = self.change_on_recheck
            self.pages[url] = raw
            self.change_on_recheck = None
        etag = digest(raw)
        status = (
            304
            if request.headers.get("If-None-Match") == etag and not self.unchanged_200
            else 200
        )
        self.requests.append((url, self.clock.now, status))
        return httpx.Response(
            status,
            headers={"Content-Type": "text/html", "ETag": etag},
            content=raw if status == 200 else b"",
        )
