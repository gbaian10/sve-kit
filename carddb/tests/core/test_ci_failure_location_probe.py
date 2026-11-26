"""Temporary synthetic failure for the public CI summary."""


def test_ci_failure_location_probe() -> None:
    """Fail with one synthetic value to verify public redaction."""
    label = "synthetic failure probe"
    wording = f"{label} ci-private-python-wording"
    assert wording == "synthetic comparison value"
