# ruff: file-ignore[no-self-use, magic-value-comparison] -- standalone unittest with synthetic exit statuses
"""Exercise the public-input wrapper without compiling or reading official data."""

import json
import os
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- synthetic process results
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import rust_tests


class RustTests(unittest.TestCase):
    def test_every_scope_regenerates_and_cleans_input(self) -> None:
        """Neither fork mode nor an inherited private path may skip real tests."""
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "tests/engine").mkdir(parents=True)
            (root / "tests/engine/card-input.json").write_text(
                json.dumps({"batch_id": "synthetic"})
            )
            for mode in ("full", "fork"):
                generated = []

                def execute(
                    argv: list[str], generated: list[Path] = generated, **kwargs: object
                ) -> subprocess.CompletedProcess[str]:
                    if argv[0] == "uv":
                        output = Path(argv[-1])
                        assert not output.exists()
                        output.write_text("synthetic", encoding="utf-8")
                        generated.append(output)
                        return subprocess.CompletedProcess(argv, 0)
                    environment = kwargs["env"]
                    assert isinstance(environment, dict)
                    assert Path(environment["SVE_TEST_SNAPSHOT"]) == generated[0]
                    assert "--test" not in argv
                    assert "--workspace" in argv
                    return subprocess.CompletedProcess(argv, 7)

                with (
                    patch.object(rust_tests, "ROOT", root),
                    patch.object(
                        rust_tests, "executable", side_effect=lambda name: name
                    ),
                    patch.object(subprocess, "run", side_effect=execute),
                    patch.dict(
                        os.environ,
                        {
                            "SVE_CI_TEST_MODE": mode,
                            "SVE_TEST_SNAPSHOT": "/unused/private",
                        },
                    ),
                ):
                    assert rust_tests.main([]) == 7
                assert not generated[0].exists()
