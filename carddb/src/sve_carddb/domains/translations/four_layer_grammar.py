"""Pinned N0 source constructions shared by partitioning and semantic guards."""

import re

TYPE_WORDS = "フォロワー|アミュレット|スペル|イクイップメント|クレスト"
TOKEN_HEADER = re.compile(
    r"^『(?P<name>[^』]+)』\{(?P<cls>[^}]+)\}(?P<kind>[^{『]*?(?:"
    + TYPE_WORDS
    + r"))(?:\{コスト(?P<cost>[^}]*)\})?"
    r"(?:\{攻撃力\}(?P<atk>[^/]*)/\{体力\}(?P<hp>[0-9０-９]+))?"
)
