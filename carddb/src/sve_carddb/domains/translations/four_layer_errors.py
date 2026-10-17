"""Absent presentation values trigger whole-field fallback."""


class MissingValueError(Exception):
    """Invalid structures still fail; only unavailable values fall back."""
