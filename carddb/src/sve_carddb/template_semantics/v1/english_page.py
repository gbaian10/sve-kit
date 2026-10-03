"""Frozen EN card-page validation delegates to the same fixed HTML boundary."""

from sve_carddb.template_semantics.v1.page import CardPage
from sve_carddb.template_semantics.v1.page import parse_card as physical_page


def parse_card(body: bytes, *, expected_number: str) -> CardPage:
    """Preserve the independently fixed EN page directory."""
    return physical_page(
        body,
        expected_number=expected_number,
        page_dir="https://en.shadowverse-evolve.com/cards/",
    )
