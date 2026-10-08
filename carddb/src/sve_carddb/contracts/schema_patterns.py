"""Construct portable calendar and URI patterns without format-checker plugins."""

from sve_carddb.core.dates import DATE, INSTANT, date_pattern


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
    compact_clock = r"(?:[01][0-9]|2[0-3])[0-5][0-9][0-5][0-9]"
    version = date_pattern("") + "T" + compact_clock + r"Z-(?!0000)[0-9]{4}"
    values = {
        "DataVersion": "(preview-)?" + version,
        "URL": _url(),
    }
    # Both regex engines allow $ before a trailing newline; require absolute end.
    return {"Date": DATE, "Instant": INSTANT} | {
        key: "^" + value + r"$(?![\s\S])" for key, value in values.items()
    }
