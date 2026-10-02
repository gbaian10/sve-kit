"""Explicit libbrotli 1.0.9 generic/q11/lgwin22 protocol for existing previews."""

import ctypes
import sys


class Encoder:
    """Load only the system library, with the preview producer's exact recipe."""

    def __init__(self) -> None:
        self.lib = ctypes.CDLL("libbrotlienc.so.1")
        self.lib.BrotliEncoderVersion.restype = ctypes.c_uint32
        if int(self.lib.BrotliEncoderVersion()) != (1 << 24) + 9:
            raise ValueError("Preview encoder requires libbrotli 1.0.9")
        self.lib.BrotliEncoderMaxCompressedSize.argtypes = [ctypes.c_size_t]
        self.lib.BrotliEncoderMaxCompressedSize.restype = ctypes.c_size_t
        self.lib.BrotliEncoderCompress.argtypes = [
            ctypes.c_int,
            ctypes.c_int,
            ctypes.c_int,
            ctypes.c_size_t,
            ctypes.c_void_p,
            ctypes.POINTER(ctypes.c_size_t),
            ctypes.c_void_p,
        ]
        self.lib.BrotliEncoderCompress.restype = ctypes.c_int

    def compress(self, raw: bytes) -> bytes:
        """Use a one-shot generic encoder without an implicit size hint or window."""
        capacity = int(self.lib.BrotliEncoderMaxCompressedSize(len(raw)))
        size = ctypes.c_size_t(capacity)
        output = ctypes.create_string_buffer(capacity)
        data = ctypes.create_string_buffer(raw, max(1, len(raw)))
        if (
            int(
                self.lib.BrotliEncoderCompress(
                    11, 22, 0, len(raw), data, ctypes.byref(size), output
                )
            )
            != 1
        ):
            raise ValueError("Preview encoding failed")
        return output.raw[: size.value]


def main() -> int:
    """Accept only the producer's --version or -q 11 -c byte-stream protocol."""
    if sys.argv[1:] not in (["--version"], ["-q", "11", "-c"]):
        sys.stderr.write("Expected --version or -q 11 -c\n")
        return 2
    try:
        encoder = Encoder()
        if sys.argv[1:] == ["--version"]:
            sys.stdout.write("svekit-preview libbrotli 1.0.9 generic q11 lgwin22\n")
        else:
            sys.stdout.buffer.write(encoder.compress(sys.stdin.buffer.read()))
    except OSError, AttributeError, ValueError:
        sys.stderr.write("Preview encoder unavailable or incompatible\n")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
