"""Static references used by pytest and cross-language consumers."""

from typing import Any

_retained: Any = None
_ = (
    _retained.unbound,  # Pytest fixture imports: tests/domains/rulings/test_mapping.py:34 and test_rulings.py:16.
    _retained.plain_cli_output,  # Pytest autouse fixture: tests/conftest.py:72.
    _retained.current_lowering_inputs,  # Python subprocess: sim/engine/src/ir/tests/lowering.rs:40.
)
