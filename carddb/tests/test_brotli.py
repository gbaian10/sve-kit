"""Real package round trips, bounded decoding and redacted failures on synthetic bytes."""

import brotli
import pytest

from sve_carddb.core.compression import verify_brotli
from sve_carddb.snapshot.export.compression import python_brotli, recipe


@pytest.mark.parametrize(
    "raw",
    [b"", b"synthetic JSON", bytes(range(256)), b"x" * 500_000],
    ids=["empty", "text", "binary", "streaming"],
)
def test_native_codec_records_version_and_roundtrips(raw: bytes) -> None:
    codec = python_brotli()
    encoded = codec.compress(raw)
    assert recipe(codec)["brotli"] == "brotli " + brotli.__version__
    assert brotli.decompress(encoded) == raw
    verify_brotli(encoded, raw)


@pytest.mark.parametrize("quality", [0, 4, 11])
def test_validation_does_not_require_producer_quality(quality: int) -> None:
    raw = b"synthetic data" * 100
    verify_brotli(brotli.compress(raw, quality=quality), raw)


@pytest.mark.parametrize(
    "damage", ["invalid", "truncated", "trailing", "wrong", "short", "bomb"]
)
def test_invalid_or_unbounded_representations_are_rejected(damage: str) -> None:
    raw = b"synthetic data" * 100
    encoded = brotli.compress(raw)
    if damage == "invalid":
        encoded = b"synthetic invalid stream"
    elif damage == "truncated":
        encoded = encoded[:-1]
    elif damage == "trailing":
        encoded += b"synthetic trailing bytes"
    elif damage == "wrong":
        raw = b"different data" * 100
    elif damage == "short":
        raw += b"more expected bytes"
    else:
        encoded = brotli.compress(b"x" * 1_000_000)
        raw = b""
    with pytest.raises(ValueError, match=r"^Invalid Brotli representation$"):
        verify_brotli(encoded, raw)


def test_native_errors_never_expose_their_message(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    def failed(*_args: object, **_kwargs: object) -> bytes:
        raise brotli.error("synthetic private sentinel")

    monkeypatch.setattr(brotli, "compress", failed)
    with pytest.raises(ValueError, match=r"^Brotli compression failed$"):
        python_brotli().compress(b"synthetic input")
