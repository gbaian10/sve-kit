"""Explicit producer compression without leaking its stderr or using a new dependency."""

import subprocess  # ruff: ignore[suspicious-subprocess-import] -- explicitly selected local encoder, never a shell
from pathlib import Path

from sve_carddb.r2_upload.plan import UploadError
from sve_carddb.snapshot.export import Brotli
from sve_carddb.snapshot.values import digest


def command_brotli(command: Path) -> Brotli:
    """Require the producer's command protocol and redact local preparation failures."""
    try:
        executable = str(command.resolve(strict=True))
        version = (
            subprocess.check_output(  # ruff: ignore[subprocess-without-shell-equals-true] -- explicit trusted encoder without a shell
                [executable, "--version"], stderr=subprocess.DEVNULL
            )
            .decode()
            .strip()
        )
        pin = version + " / " + digest(Path(executable).read_bytes())
    except OSError, UnicodeError, subprocess.SubprocessError:
        raise UploadError("Explicit Brotli compressor failed") from None

    def compress(raw: bytes) -> bytes:
        try:
            return subprocess.check_output(  # ruff: ignore[subprocess-without-shell-equals-true] -- pinned local encoder; stderr is discarded
                [executable, "-q", "11", "-c"], input=raw, stderr=subprocess.DEVNULL
            )
        except OSError, subprocess.SubprocessError:
            raise UploadError("Explicit Brotli compressor failed") from None

    return Brotli(pin, compress)
