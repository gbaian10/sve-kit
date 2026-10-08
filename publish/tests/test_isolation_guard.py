"""Independent session safety survives per-test patch teardown."""

import importlib.util
import os
import socket
import sys
import tempfile
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from threading import Thread
from typing import TYPE_CHECKING, override

import httpx
import pytest

from .isolation_guard import IsolationGuard
from .isolation_guard import TestIsolationError as IsolationError

if TYPE_CHECKING:
    from collections.abc import Iterator
URL = "https://example.invalid/test-only"


def forbidden_root(guard: IsolationGuard, tmp_path: Path) -> Path:
    # The sentinel is outside the approved root but still under the system test directory.
    return guard.temporary_root.parent / f"{tmp_path.name}-not-approved-data"


def test_standard_library_tempdirs_stay_isolated_after_undo(
    test_isolation_guard: IsolationGuard,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(
        tempfile, "tempdir", str(test_isolation_guard.temporary_root / "unused")
    )
    monkeypatch.undo()
    with tempfile.TemporaryDirectory() as temporary:
        assert os.path.commonpath(
            [temporary, str(test_isolation_guard.temporary_root)]
        ) == str(test_isolation_guard.temporary_root)


def test_sync_default_http_is_blocked_after_test_undo(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setenv("SVE_GUARD_SYNTHETIC", "temporary")
    monkeypatch.undo()
    with (
        httpx.Client(trust_env=False) as client,
        pytest.raises(IsolationError, match="external HTTP"),
    ):
        client.get(URL)


async def test_async_default_http_is_blocked_after_test_undo(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setenv("SVE_GUARD_SYNTHETIC", "temporary")
    monkeypatch.undo()
    async with httpx.AsyncClient(trust_env=False) as client:
        with pytest.raises(IsolationError, match="external HTTP"):
            await client.get(URL)


def test_mock_transport_remains_available() -> None:
    with httpx.Client(
        transport=httpx.MockTransport(
            lambda _request: httpx.Response(200, content=b"synthetic")
        )
    ) as client:
        assert client.get(URL).content == b"synthetic"


@pytest.mark.parametrize(
    ("event", "args"),
    [
        ("socket.getaddrinfo", ("example.invalid", 443, 0, 0, 0)),
        ("socket.gethostbyname", ("example.invalid",)),
        ("socket.gethostbyaddr", ("203.0.113.1",)),
        ("socket.connect", (None, ("203.0.113.1", 443))),
    ],
)
def test_native_network_audit_events_stay_blocked_after_undo(
    monkeypatch: pytest.MonkeyPatch,
    event: str,
    args: tuple[object, ...],
) -> None:
    monkeypatch.setenv("SVE_GUARD_SYNTHETIC", "temporary")
    monkeypatch.undo()
    # Emit the exact native event without risking a connection if this assertion regresses.
    with pytest.raises(IsolationError):
        sys.audit(event, *args)


def test_real_external_socket_connect_is_blocked_before_system_call() -> None:
    with (
        socket.socket() as connection,
        pytest.raises(IsolationError, match="external socket"),
    ):
        connection.connect(("203.0.113.1", 443))


def test_real_external_dns_is_blocked_before_resolution() -> None:
    with pytest.raises(IsolationError, match="external name"):
        socket.getaddrinfo("example.invalid", 443)


def test_cold_import_outside_data_root_can_create_bytecode(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    source = tmp_path / "external-module.py"
    source.write_text("SYNTHETIC = True\n", encoding="utf-8")
    assert not (source.parent / "__pycache__").exists()
    approved = tmp_path / "data"
    approved.mkdir()
    monkeypatch.setenv("SVE_DATA_DIR", str(approved))
    guard = IsolationGuard(approved, tmp_path / "coverage")
    # A narrower nested guard reproduces a cold external import without touching real files.
    sys.addaudithook(guard.audit)
    try:
        spec = importlib.util.spec_from_file_location("guard_cold_import", source)
        assert spec is not None
        assert spec.loader is not None
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        assert module.SYNTHETIC is True
        assert spec.cached is not None
        assert source.parent / "__pycache__" in Path(spec.cached).parents
        assert Path(spec.cached).is_file()
        with pytest.raises(IsolationError):
            (source.parent / "__pycache__" / "manifest.sqlite").write_bytes(
                b"synthetic"
            )
    finally:
        guard._active = False


class LocalHandler(BaseHTTPRequestHandler):
    def do_GET(self) -> None:
        self.send_response(200)
        self.end_headers()
        self.wfile.write(b"localhost synthetic server")

    @override
    def log_message(self, _format: str, *_args: object) -> None:
        pass


@pytest.fixture(scope="module")
def localhost_server() -> Iterator[str]:
    server = ThreadingHTTPServer(("127.0.0.1", 0), LocalHandler)
    thread = Thread(target=server.serve_forever, kwargs={"poll_interval": 0.01})
    thread.start()
    try:
        yield f"http://127.0.0.1:{server.server_port}/"
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=1)
        assert not thread.is_alive()


@pytest.mark.parametrize("directory", ["htmlcov", "__pycache__"])
def test_artifact_exemptions_do_not_allow_manifest_paths(
    test_isolation_guard: IsolationGuard,
    directory: str,
) -> None:
    path = test_isolation_guard.coverage_root.parent / directory / "manifest.sqlite"
    with pytest.raises(IsolationError):
        sys.audit("sqlite3.connect", path)


def test_localhost_real_transport_is_allowed(localhost_server: str) -> None:
    with httpx.Client(trust_env=False) as client:
        assert client.get(localhost_server).content == b"localhost synthetic server"


async def test_async_localhost_real_transport_is_allowed(localhost_server: str) -> None:
    async with httpx.AsyncClient(trust_env=False) as client:
        assert (
            await client.get(localhost_server)
        ).content == b"localhost synthetic server"


@pytest.mark.parametrize("mode", ["missing", "outside"])
def test_invalid_environment_blocks_writes(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    test_isolation_guard: IsolationGuard,
    mode: str,
) -> None:
    if mode == "missing":
        monkeypatch.delenv("SVE_DATA_DIR")
    else:
        monkeypatch.setenv(
            "SVE_DATA_DIR", str(forbidden_root(test_isolation_guard, tmp_path))
        )
    with pytest.raises(IsolationError, match="SVE_DATA_DIR"):
        (tmp_path / "untouched").write_bytes(b"synthetic")


def test_external_write_and_symlink_target_are_blocked(
    tmp_path: Path, test_isolation_guard: IsolationGuard
) -> None:
    outside = forbidden_root(test_isolation_guard, tmp_path)
    with pytest.raises(IsolationError):
        outside.write_bytes(b"synthetic")
    alias = tmp_path / "alias"
    alias.symlink_to(outside)
    with pytest.raises(IsolationError):
        alias.write_bytes(b"synthetic")
    alias.unlink()


@pytest.mark.skipif(
    os.unlink not in os.supports_dir_fd,
    reason="Directory-relative unlink is unavailable",
)
def test_tracked_directory_descriptor_cannot_escape(
    tmp_path: Path, test_isolation_guard: IsolationGuard
) -> None:
    root = tmp_path / "directory"
    root.mkdir()
    member = root / "member"
    member.write_bytes(b"synthetic")
    descriptor = os.open(root, os.O_RDONLY)
    try:
        os.unlink("member", dir_fd=descriptor)
        assert not member.exists()
        with pytest.raises(IsolationError):
            os.unlink(
                str(forbidden_root(test_isolation_guard, tmp_path)), dir_fd=descriptor
            )
    finally:
        os.close(descriptor)
