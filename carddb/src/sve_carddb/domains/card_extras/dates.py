"""Shared observed English date grammar for card pages and announcements."""

import re
from datetime import date

_EN_DATE = re.compile(r"([A-Z][a-z]{2})(?:\. ([0-9]{1,2})| ([0-9]{2})), ([0-9]{4})")
_EN_MONTHS = {
    month: number
    for number, month in enumerate(
        (
            "Jan",
            "Feb",
            "Mar",
            "Apr",
            "May",
            "Jun",
            "Jul",
            "Aug",
            "Sep",
            "Oct",
            "Nov",
            "Dec",
        ),
        start=1,
    )
}


def parse_en_date(raw: str | None) -> str | None:
    """Parse only the observed English month forms, without guessing unknown dates."""
    match = _EN_DATE.fullmatch(raw or "")
    if match is None or (month := _EN_MONTHS.get(match[1])) is None:
        return None
    try:
        return date(int(match[4]), month, int(match[2] or match[3])).isoformat()
    except ValueError:
        return None
