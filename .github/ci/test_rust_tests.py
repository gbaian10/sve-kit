# ruff: file-ignore[no-self-use, magic-value-comparison] -- standalone unittest with synthetic exit statuses
"""Exercise the public-input wrapper without compiling or reading official data."""

import os
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- synthetic process results
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import rust_tests


class RustTests(unittest.TestCase):
    def test_regenerates_and_cleans_input(self) -> None:
        """An inherited snapshot path cannot skip generation or cleanup."""
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for arguments in (
                [],
                ["mutants", "--workspace", "-j", "4", "--timeout", "120"],
            ):
                generated: list[Path] = []

                def execute(
                    argv: list[str],
                    generated: list[Path] = generated,
                    arguments: list[str] = arguments,
                    **kwargs: object,
                ) -> subprocess.CompletedProcess[str]:
                    if argv[0] == "uv":
                        for filename in argv[-2:]:
                            output = Path(filename)
                            assert output.is_absolute()
                            assert not output.exists()
                            output.write_text("synthetic", encoding="utf-8")
                            generated.append(output)
                        return subprocess.CompletedProcess(argv, 0)
                    environment = kwargs["env"]
                    assert isinstance(environment, dict)
                    assert Path(environment["SVE_TEST_SNAPSHOT"]) == generated[0]
                    assert (
                        Path(environment["SVE_TEST_CURRENT_SELECTION"]) == generated[1]
                    )
                    assert argv == [
                        "mise",
                        "exec",
                        "--",
                        "cargo",
                        *(arguments or rust_tests.DEFAULT),
                    ]
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
                            "SVE_TEST_SNAPSHOT": "/unused/inherited-snapshot",
                            "SVE_TEST_CURRENT_SELECTION": "/unused/inherited-selection",
                        },
                    ),
                ):
                    assert rust_tests.main(arguments) == 7
                assert not generated[0].exists()
                assert not generated[1].exists()
