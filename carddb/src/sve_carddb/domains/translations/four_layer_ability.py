"""Shared lexical boundaries for the integrated source families."""

import re

ABILITY_HEAD = (
    r"\{(?:ファンファーレ|起動|アドバンス起動|ラストワード)\}"
    r"|\{(?:進化|食事|憑依)\}(?=\{コスト)"
    r"|【(?:攻撃時|進化時|超進化時|出走時|ドライブ獲得時)】"
)
OPTION_LABEL = r"【[0-9０-９N]+】"
HEAD = re.compile(ABILITY_HEAD)
