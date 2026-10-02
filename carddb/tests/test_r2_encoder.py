"""Synthetic C-library protocol tests; no platform libbrotli dependency in pytest."""

import ctypes
import sys
from dataclasses import dataclass, field
from io import BytesIO
from types import SimpleNamespace

import pytest

from sve_carddb.r2_upload.brotli_encoder import Encoder, main


@dataclass
class Function:
    value: int
    argtypes: object = None
    restype: object = None
    calls: list[tuple[object, ...]] = field(default_factory=list)

    def __call__(self, *args: object) -> int:
        self.calls.append(args)
        return self.value


@pytest.fixture
def library(monkeypatch: pytest.MonkeyPatch) -> SimpleNamespace:
    fake = SimpleNamespace(
        BrotliEncoderVersion=Function((1 << 24) + 9),
        BrotliEncoderMaxCompressedSize=Function(2),
        BrotliEncoderCompress=Function(1),
    )

    def load(path: str) -> SimpleNamespace:
        assert path == "libbrotlienc.so.1"
        return fake

    monkeypatch.setattr(ctypes, "CDLL", load)
    return fake


def test_wrapper_uses_exact_quality_window_mode_and_binary_protocol(
    library: SimpleNamespace,
) -> None:
    encoder = Encoder()
    assert encoder.compress(b"abc") == b"\0\0"
    args = library.BrotliEncoderCompress.calls[0]
    assert args[:4] == (11, 22, 0, 3)
    assert ctypes.string_at(args[4], 3) == b"abc"
    assert library.BrotliEncoderCompress.restype is ctypes.c_int


def test_wrapper_checks_encoder_failure(library: SimpleNamespace) -> None:
    library.BrotliEncoderCompress.value = 0
    with pytest.raises(ValueError, match=r"^Preview encoding failed$"):
        Encoder().compress(b"abc")


def test_wrapper_rejects_another_library_version(library: SimpleNamespace) -> None:
    library.BrotliEncoderVersion.value = (1 << 24) + 8
    with pytest.raises(ValueError, match=r"^Preview encoder requires libbrotli 1.0.9$"):
        Encoder()


def test_version_cli(
    library: SimpleNamespace,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    monkeypatch.setattr(sys, "argv", ["encoder", "--version"])
    assert main() == 0
    assert (
        capsys.readouterr().out
        == "svekit-preview libbrotli 1.0.9 generic q11 lgwin22\n"
    )
    assert library.BrotliEncoderCompress.calls == []


def test_stream_cli(library: SimpleNamespace, monkeypatch: pytest.MonkeyPatch) -> None:
    output = BytesIO()
    monkeypatch.setattr(sys, "argv", ["encoder", "-q", "11", "-c"])
    monkeypatch.setattr(sys, "stdin", SimpleNamespace(buffer=BytesIO(b"abc")))
    monkeypatch.setattr(sys, "stdout", SimpleNamespace(buffer=output))
    assert main() == 0
    assert output.getvalue() == b"\0\0"
    assert library.BrotliEncoderCompress.calls[0][:4] == (11, 22, 0, 3)


def test_invalid_cli_never_loads_library(
    monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    def forbidden(_path: str) -> None:
        pytest.fail("Invalid arguments must not load a library")

    monkeypatch.setattr(ctypes, "CDLL", forbidden)
    monkeypatch.setattr(sys, "argv", ["encoder", "-q", "10", "-c"])
    assert main() == 2
    assert capsys.readouterr().err == "Expected --version or -q 11 -c\n"


def test_unavailable_library_is_redacted(
    monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    def unavailable(_path: str) -> None:
        raise OSError("/private/synthetic/library")

    monkeypatch.setattr(ctypes, "CDLL", unavailable)
    monkeypatch.setattr(sys, "argv", ["encoder", "--version"])
    assert main() == 1
    assert capsys.readouterr().err == "Preview encoder unavailable or incompatible\n"
