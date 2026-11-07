"""Held-out evaluation: deck photos posted within 8 days from the start of these events never feed training,
labels for detector training, or any texture bank (folder names under SVE_SCAN_DATA/web-x/sample-events-orig,
a `_p2` suffix marks a second search page of the same event)."""

EVAL_EVENTS = frozenset(
    {
        "2025-11-15_GP_大阪",
        "2024-08-10_GP_名古屋",
        "2026-10-24_GP_千葉",
        "2025-01-25_GP_京都",
        "2024-05-03_GP_東京",
        "2026-05-09_GCS_静岡",
    }
)


def is_eval(event: str) -> bool:
    return event.removesuffix("_p2") in EVAL_EVENTS
