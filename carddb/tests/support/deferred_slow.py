"""Real-repository tests paused until they are made cheap enough for CI (#1104)."""

REASON = "Paused until the real-repository test is made cheaper (#1104)"

DEFERRED: frozenset[str] = frozenset()


def is_deferred(nodeid: str) -> bool:
    """Match a collected test, ignoring its parameter suffix."""
    return nodeid.split("[", 1)[0] in DEFERRED
