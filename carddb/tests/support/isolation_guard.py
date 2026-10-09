"""Process-wide backstops for test data and networking, independent of test patches."""

import ipaddress
import os
import socket
from pathlib import Path
from typing import TYPE_CHECKING
from urllib.parse import unquote, urlsplit

import httpx

from sve_carddb.ingest.archive.manifest import Manifest
from sve_carddb.ingest.http.writer import Writer

if TYPE_CHECKING:
    from collections.abc import Callable

    import pytest

    from sve_carddb.ingest.archive.manifest import Resource
    from sve_carddb.ingest.http.writer import Fetched, WriteResult


class TestIsolationError(RuntimeError):
    """A test attempted to access non-test data or the external network."""


def _loopback(host: object) -> bool:
    if host == "localhost":
        return True
    if not isinstance(host, str):
        return False
    try:
        return ipaddress.ip_address(host).is_loopback
    except ValueError:
        return False


class IsolationGuard:
    """Audit hooks cannot be undone by an individual test's MonkeyPatch.undo()."""

    def __init__(self, temporary_root: Path, coverage_root: Path) -> None:
        self.temporary_root = temporary_root.resolve()
        self.coverage_root = coverage_root.resolve()
        self._active = True

    def _path(
        self, value: object, dir_fd: object = None, *, entry: bool = False
    ) -> Path | None:
        if isinstance(value, int):
            return None
        if not isinstance(value, str | bytes | os.PathLike):
            raise TestIsolationError("test isolation: unsupported filesystem path")
        path = Path(os.fsdecode(value))
        if not path.is_absolute() and isinstance(dir_fd, int) and dir_fd >= 0:
            path = Path(f"/proc/self/fd/{dir_fd}").resolve(strict=True) / path
        return path.parent.resolve() / path.name if entry else path.resolve()

    def _environment(self) -> None:
        value = os.environ.get("SVE_DATA_DIR")
        if not value:
            raise TestIsolationError(
                "test isolation: SVE_DATA_DIR must be explicitly temporary"
            )
        if not Path(value).resolve().is_relative_to(self.temporary_root):
            raise TestIsolationError(
                "test isolation: SVE_DATA_DIR is outside the pytest temporary root"
            )

    def _write_path(
        self, value: object, dir_fd: object = None, *, entry: bool = False
    ) -> None:
        path = self._path(value, dir_fd, entry=entry)
        if path is None:
            return
        if path == self.coverage_root or (
            path.parent == self.coverage_root and path.name.startswith(".coverage")
        ):
            return
        # Importlib atomically writes cache.pyc.<id> before replacing cache.pyc.
        bytecode = path.name.endswith(".pyc") or (
            ".pyc." in path.name and path.name.rsplit(".pyc.", 1)[1].isdecimal()
        )
        if "__pycache__" in path.parts and (path.name == "__pycache__" or bytecode):
            return
        self._environment()
        if not path.is_relative_to(self.temporary_root):
            raise TestIsolationError(
                "test isolation: writing outside the pytest temporary root"
            )

    def _database(self, value: object) -> None:
        if value == ":memory:":
            return
        if isinstance(value, str) and value.startswith("file:"):
            parts = urlsplit(value)
            if parts.path == ":memory:" or "mode=memory" in parts.query.split("&"):
                return
            value = unquote(parts.path)
        self._write_path(value)

    def _source_path(self, path: Path) -> None:
        self._environment()
        if not path.resolve().is_relative_to(self.temporary_root):
            raise TestIsolationError(
                "test isolation: source path is outside the pytest temporary root"
            )

    def protect_sources(self, patch: pytest.MonkeyPatch) -> None:
        """Validate source entry points even when no filesystem write would occur."""

        def wrap_open(
            opener: Callable[[Path], Manifest],
        ) -> classmethod[Manifest, [Path], Manifest]:
            def guarded(_cls: type[Manifest], path: Path) -> Manifest:
                self._source_path(path)
                return opener(path)

            return classmethod(guarded)

        for name in ("open", "open_live", "open_snapshot"):
            patch.setattr(Manifest, name, wrap_open(getattr(Manifest, name)))
        empty = Manifest.open_empty

        def open_empty(_cls: type[Manifest]) -> Manifest:
            self._environment()
            return empty()

        patch.setattr(Manifest, "open_empty", classmethod(open_empty))
        original = Writer.write

        def write(
            writer: Writer,
            fetched: Fetched,
            *,
            request_id: int,
            rewrite: bool = False,
            in_transaction: Callable[[Resource], None] | None = None,
        ) -> WriteResult:
            self._source_path(writer._root)
            return original(
                writer,
                fetched,
                request_id=request_id,
                rewrite=rewrite,
                in_transaction=in_transaction,
            )

        patch.setattr(Writer, "write", write)

    def audit(self, event: str, args: tuple[object, ...]) -> None:  # ruff: ignore[complex-structure] -- native event shapes are kept together for review
        """Reject I/O before the native SQLite, filesystem or socket operation occurs."""
        if not self._active:
            return
        if event == "sqlite3.connect":
            self._database(args[0])
        elif event == "open":
            mode, flags = args[1:3]
            writing = isinstance(mode, str) and any(char in mode for char in "wax+")
            writing = writing or (
                isinstance(flags, int)
                and bool(
                    flags
                    & (os.O_WRONLY | os.O_RDWR | os.O_CREAT | os.O_TRUNC | os.O_APPEND)
                )
            )
            if writing:
                self._write_path(args[0])
        elif event in {"os.remove", "os.rmdir"}:
            self._write_path(args[0], args[1], entry=True)
        elif event == "os.mkdir":
            self._write_path(args[0], args[2])
        elif event in {"os.chmod", "os.truncate", "os.utime"}:
            self._write_path(args[0])
        elif event in {"os.rename", "os.link", "os.symlink"}:
            self._write_path(args[1], args[-1], entry=True)
            if event != "os.symlink":
                self._write_path(args[0], args[2], entry=True)
        elif event in {"socket.connect", "socket.sendto", "socket.sendmsg"} or (
            event == "socket.bind"
            and isinstance(args[0], socket.socket)
            and args[0].family == socket.AF_UNIX
        ):
            self._address(args[0], args[1])
        elif event in {
            "socket.getaddrinfo",
            "socket.gethostbyname",
            "socket.gethostbyaddr",
        } and not _loopback(args[0]):
            raise TestIsolationError(
                "test isolation: external name resolution is forbidden"
            )

    def _address(self, connection: object, address: object) -> None:
        connected = address is None and isinstance(connection, socket.socket)
        if connected:
            assert isinstance(connection, socket.socket)
            address = connection.getpeername()
        if (
            isinstance(connection, socket.socket)
            and connection.family == socket.AF_UNIX
        ):
            # Unnamed socketpair peers are local and have no filesystem destination.
            if connected and address in {"", b""}:
                return
            if (
                not isinstance(address, str | bytes)
                or not address
                or "\0" in os.fsdecode(address)
            ):
                raise TestIsolationError(
                    "test isolation: unsupported Unix socket address"
                )
            self._source_path(Path(os.fsdecode(address)))
            return
        if not isinstance(address, tuple) or not address or not _loopback(address[0]):
            raise TestIsolationError(
                "test isolation: external socket connections are forbidden"
            )

    def protect_http(self, patch: pytest.MonkeyPatch) -> None:
        """Allow MockTransport and localhost, rejecting default transports before DNS."""
        sync = httpx.HTTPTransport.handle_request
        asynchronous = httpx.AsyncHTTPTransport.handle_async_request

        def handle(
            transport: httpx.HTTPTransport, request: httpx.Request
        ) -> httpx.Response:
            if not _loopback(request.url.host):
                raise TestIsolationError(
                    "test isolation: external HTTP transport is forbidden"
                )
            return sync(transport, request)

        async def handle_async(
            transport: httpx.AsyncHTTPTransport, request: httpx.Request
        ) -> httpx.Response:
            if not _loopback(request.url.host):
                raise TestIsolationError(
                    "test isolation: external HTTP transport is forbidden"
                )
            return await asynchronous(transport, request)

        patch.setattr(httpx.HTTPTransport, "handle_request", handle)
        patch.setattr(httpx.AsyncHTTPTransport, "handle_async_request", handle_async)
