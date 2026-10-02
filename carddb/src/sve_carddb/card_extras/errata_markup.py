"""Retain original HTML byte ranges independently of DOM repair or rendering."""

from dataclasses import dataclass, field
from html import unescape
from html.parser import HTMLParser
from typing import Literal, override


@dataclass(frozen=True)
class Piece:
    kind: Literal["text", "image", "break", "heading"]
    start: int
    end: int
    value: str = field(repr=False, default="")
    attrs: tuple[tuple[str, str | None], ...] = field(repr=False, default=())
    spans: tuple[int, ...] = ()


class NoticeMarkup(HTMLParser):
    def __init__(self, html: str) -> None:
        super().__init__(convert_charrefs=False)
        self.html = html
        self.offsets = [0]
        for position, character in enumerate(html):
            if character == "\n":
                self.offsets.append(position + 1)
        self.stack: list[tuple[str, dict[str, str | None], int]] = []
        self.pieces: list[Piece] = []
        self.bodies = 0

    def position(self) -> int:
        """Translate parser line/column positions without serializing a repaired DOM."""
        line, column = self.getpos()
        return self.offsets[line - 1] + column

    def active(self) -> bool:
        """Exclude embedded scripts/styles even when they contain pair markers."""
        return any(
            "contents" in (attrs.get("class") or "").split()
            and "sw-Txtarea" in (attrs.get("class") or "").split()
            for _, attrs, _ in self.stack
        ) and not any(tag in {"script", "style"} for tag, _, _ in self.stack)

    def emit(
        self,
        kind: Literal["text", "image", "break", "heading"],
        value: str = "",
        attrs: tuple[tuple[str, str | None], ...] = (),
        length: int = 0,
    ) -> None:
        """Keep lexical offsets so rendering cannot change the evidence identity."""
        if self.active():
            start = self.position()
            self.pieces.append(
                Piece(
                    kind,
                    start,
                    start + length,
                    value,
                    attrs,
                    tuple(pos for tag, _, pos in self.stack if tag == "span"),
                )
            )

    @override
    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        classes = (dict(attrs).get("class") or "").split()
        if "contents" in classes and "sw-Txtarea" in classes:
            self.bodies += 1
        if tag in {"p", "div", "section", "h1", "h2", "h3", "h4", "li"}:
            self.emit("break")
        if tag == "section" and "inh1" in classes:
            self.emit("heading")
        if tag == "img":
            self.emit(
                "image", attrs=tuple(attrs), length=len(self.get_starttag_text() or "")
            )
        if tag == "br":
            self.emit("break")
        if tag not in {"img", "br", "hr", "meta", "link", "input", "wbr"}:
            self.stack.append((tag, dict(attrs), self.position()))

    @override
    def handle_startendtag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        self.handle_starttag(tag, attrs)
        if self.stack and self.stack[-1][0] == tag:
            self.stack.pop()

    @override
    def handle_endtag(self, tag: str) -> None:
        if tag in {"p", "div", "section", "h1", "h2", "h3", "h4", "li"}:
            self.emit("break")
        for index in range(len(self.stack) - 1, -1, -1):
            if self.stack[index][0] == tag:
                del self.stack[index:]
                break

    @override
    def handle_data(self, data: str) -> None:
        self.emit("text", data, length=len(data))

    @override
    def handle_entityref(self, name: str) -> None:
        self._reference("&" + name)

    @override
    def handle_charref(self, name: str) -> None:
        self._reference("&#" + name)

    def _reference(self, value: str) -> None:
        # HTMLParser accepts references without a trailing semicolon.
        if self.html.startswith(value + ";", self.position()):
            value += ";"
        self.emit("text", unescape(value), length=len(value))


def lines(pieces: list[Piece]) -> list[tuple[Piece, ...]]:
    """Discard layout-only lines without modifying retained text node values."""
    result: list[tuple[Piece, ...]] = []
    current: list[Piece] = []
    for piece in pieces:
        if piece.kind in {"break", "heading"}:
            if current:
                result.append(tuple(current))
                current = []
            if piece.kind == "heading":
                result.append((piece,))
        else:
            current.append(piece)
    if current:
        result.append(tuple(current))
    return result
