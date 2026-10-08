"""Lexical domains for build database columns."""

HASH = r"sha256:[0-9a-f]{64}"
CODE = r"[a-z][a-z0-9_-]*"
LANG = r"[a-z]{2,3}(?:-[A-Za-z0-9]{2,8})*"
