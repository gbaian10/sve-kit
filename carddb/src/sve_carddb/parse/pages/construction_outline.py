"""Unreviewed HTML outlines for construction-source adapter development."""

from dataclasses import dataclass
from typing import Literal

from sve_carddb.parse.html import attribute, parse, require_one, select_all


@dataclass(frozen=True, slots=True)
class OutlineLink:
    href: str
    label: str


@dataclass(frozen=True, slots=True)
class OutlineBlock:
    tag: str
    text: str


@dataclass(frozen=True, slots=True)
class ConstructionOutline:
    title: str
    blocks: tuple[OutlineBlock, ...]
    links: tuple[OutlineLink, ...]
    semantic_state: Literal["unreviewed"] = "unreviewed"


def parse_construction_outline(body: bytes) -> ConstructionOutline:
    """Extract observations only; official selectors and semantic recipes await a pilot."""
    document = parse(body.decode("utf-8"))
    title = require_one(document, "title").text().strip()
    content = require_one(document, "main, article")
    if not title or not content.text().strip():
        msg = "construction source lacks a nonempty title or content"
        raise ValueError(msg)
    blocks = tuple(
        OutlineBlock(tag, node.text().strip())
        for node in select_all(content, "h1, h2, h3, p, li")
        if (tag := node.tag) is not None
    )
    links = tuple(
        OutlineLink(href, node.text().strip())
        for node in select_all(content, "a[href]")
        if (href := attribute(node, "href")) is not None
    )
    return ConstructionOutline(title, blocks, links)
