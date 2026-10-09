"""Typed access to selectolax.

Always go through these helpers instead of calling `css_first` directly: its stub types
`css_first(query)` as `LexborNode`, but it returns `None` when nothing matches.
"""

from selectolax.lexbor import LexborHTMLParser, LexborNode

type Queryable = LexborHTMLParser | LexborNode


class MissingElementError(LookupError):
    """A required element or attribute is not on the page."""


def parse(html: str) -> LexborHTMLParser:
    """Parse an HTML document."""
    return LexborHTMLParser(html)


def select_one(root: Queryable, query: str) -> LexborNode | None:
    """Return the first match, or `None` when nothing matches."""
    # strict=False is the only overload typed as optional; see the module docstring.
    return root.css_first(query, strict=False)


def select_all(root: Queryable, query: str) -> list[LexborNode]:
    """Return all matches in document order."""
    return root.css(query)


def require_one(root: Queryable, query: str) -> LexborNode:
    """Return the first match, or raise `MissingElementError`."""
    node = select_one(root, query)
    if node is None:
        msg = f"no element matches {query!r}"
        raise MissingElementError(msg)
    return node


def attribute(node: LexborNode, name: str) -> str | None:
    """Return an attribute value, or `None` when it is absent."""
    return node.attributes.get(name)
