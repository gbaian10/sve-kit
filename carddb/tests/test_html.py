from typing import assert_type

import pytest
from selectolax.lexbor import LexborNode

from sve_carddb.parse.html import (
    MissingElementError,
    attribute,
    parse,
    require_attribute,
    require_one,
    select_all,
    select_one,
)

PAGE = """
<div class="card">
  <p class="number">BP01-001</p>
  <img src="/img/a.png"><img src="/img/b.png">
  <a>no href</a>
</div>
"""


def test_select_one_is_optional_and_returns_none_when_missing() -> None:
    tree = parse(PAGE)
    assert_type(select_one(tree, "span"), LexborNode | None)
    assert select_one(tree, "span") is None


def test_select_one_returns_first_match() -> None:
    node = select_one(parse(PAGE), "img")
    assert node is not None
    assert attribute(node, "src") == "/img/a.png"


def test_select_all_keeps_document_order() -> None:
    nodes = select_all(parse(PAGE), "img")
    assert [attribute(n, "src") for n in nodes] == ["/img/a.png", "/img/b.png"]


def test_require_one_raises_when_missing() -> None:
    with pytest.raises(MissingElementError, match="span"):
        require_one(parse(PAGE), "span")


def test_require_one_works_on_nodes() -> None:
    card = require_one(parse(PAGE), ".card")
    assert require_one(card, ".number").text(strip=True) == "BP01-001"


def test_attribute_is_optional() -> None:
    link = require_one(parse(PAGE), "a")
    assert_type(attribute(link, "href"), str | None)
    assert attribute(link, "href") is None


def test_require_attribute_raises_when_missing() -> None:
    link = require_one(parse(PAGE), "a")
    with pytest.raises(MissingElementError, match="href"):
        require_attribute(link, "href")
