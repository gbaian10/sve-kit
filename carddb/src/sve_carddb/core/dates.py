"""Portable calendar patterns shared by strict models and JSON contracts."""


def date_pattern(separator: str) -> str:
    """Match Gregorian dates with an explicit separator and leap-year handling."""
    year = r"(?!0000)[0-9]{4}"
    leap = (
        r"(?:[0-9]{2}(?:0[48]|[2468][048]|[13579][26])"
        r"|(?:0[48]|[2468][048]|[13579][26])00)"
    )
    return (
        rf"(?:{year}{separator}(?:"
        rf"(?:01|03|05|07|08|10|12){separator}(?:0[1-9]|[12][0-9]|3[01])|"
        rf"(?:04|06|09|11){separator}(?:0[1-9]|[12][0-9]|30)|"
        rf"02{separator}(?:0[1-9]|1[0-9]|2[0-8]))|"
        rf"{leap}{separator}02{separator}29)"
    )


_CLOCK = r"(?:[01][0-9]|2[0-3]):[0-5][0-9]:[0-5][0-9]"
DATE = "^" + date_pattern("-") + r"$(?![\s\S])"
INSTANT = "^" + date_pattern("-") + "T" + _CLOCK + r"(?:\.[0-9]+)?Z$(?![\s\S])"
