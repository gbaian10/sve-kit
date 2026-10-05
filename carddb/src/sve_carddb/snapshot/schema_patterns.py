"""Construct portable calendar and URI patterns without format-checker plugins."""


def _date(separator: str) -> str:
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


def _ipv6() -> str:
    hextet = r"[0-9A-Fa-f]{1,4}"
    octet = r"(?:25[0-5]|2[0-4][0-9]|1[0-9]{2}|[1-9]?[0-9])"
    ipv4 = rf"(?:{octet}\.){{3}}{octet}"
    last = rf"(?:{hextet}:{hextet}|{ipv4})"
    branches = [
        rf"(?:{hextet}:){{6}}{last}",
        rf"::(?:{hextet}:){{5}}{last}",
        rf"(?:{hextet})?::(?:{hextet}:){{4}}{last}",
    ]
    tails = [
        rf"(?:{hextet}:){{3}}{last}",
        rf"(?:{hextet}:){{2}}{last}",
        rf"{hextet}:{last}",
        last,
        hextet,
        "",
    ]
    branches.extend(
        rf"(?:(?:{hextet}:){{0,{count}}}{hextet})?::{tail}"
        for count, tail in enumerate(tails, 1)
    )
    return "|".join(branches)


def _url() -> str:
    unreserved = r"[A-Za-z0-9._~-]"
    sub_delimiters = r"[!$&'()*+,;=]"
    registered = rf"(?:{unreserved}|%[0-9A-Fa-f]{{2}}|{sub_delimiters})"
    segment = rf"(?:{registered}|[:@])"
    userinfo = rf"(?:{registered}|:)"
    future = rf"[vV][0-9A-Fa-f]+\.(?:{unreserved}|{sub_delimiters}|:)+"
    host = rf"(?:{registered}+|\[(?:{_ipv6()}|{future})\])"
    return (
        rf"https://(?:{userinfo}*@)?{host}(?::[0-9]*)?"
        rf"(?:/{segment}*)*(?:\?(?:{segment}|[/?])*)?"
        rf"(?:#(?:{segment}|[/?])*)?"
    )


def patterns() -> dict[str, str]:
    """Return ASCII patterns that have the same Python and ECMAScript meaning."""
    clock = r"(?:[01][0-9]|2[0-3]):[0-5][0-9]:[0-5][0-9]"
    compact_clock = r"(?:[01][0-9]|2[0-3])[0-5][0-9][0-5][0-9]"
    version = _date("") + "T" + compact_clock + r"Z-(?!0000)[0-9]{4}"
    values = {
        "Date": _date("-"),
        "Instant": _date("-") + "T" + clock + r"(?:\.[0-9]+)?Z",
        "DataVersion": "(preview-)?" + version,
        "URL": _url(),
    }
    # Both regex engines allow $ before a trailing newline; require absolute end.
    return {key: "^" + value + r"$(?![\s\S])" for key, value in values.items()}
