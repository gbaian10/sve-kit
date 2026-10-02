import importlib.util
import os
import socket
import sqlite3
import sys
import tempfile
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from threading import Thread
from typing import TYPE_CHECKING, override

import httpx
import pytest
from typer.testing import CliRunner

from sve_carddb import cli
from sve_carddb.config import Settings
from sve_carddb.fetch.writer import Fetched, Writer
from sve_carddb.manifest import Kind, Manifest, Region, RequestStart
from sve_carddb.store import relpath

from .isolation_guard import IsolationGuard
from .isolation_guard import TestIsolationError as IsolationError

if TYPE_CHECKING:
    from collections.abc import Iterator

URL = "https://example.invalid/test-only"


def forbidden_root(guard: IsolationGuard, tmp_path: Path) -> Path:
    # The sentinel is outside the approved root but still under the system test directory.
    return guard.temporary_root.parent / f"{tmp_path.name}-not-approved-data"


def test_default_environment_is_isolated(test_isolation_guard: IsolationGuard) -> None:
    assert (
        Settings()
        .data_dir.resolve()
        .is_relative_to(test_isolation_guard.temporary_root)
    )


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


def test_undo_cannot_release_guard_before_cli_opens_manifest_or_requests(
    test_isolation_guard: IsolationGuard,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    outside = forbidden_root(test_isolation_guard, tmp_path)
    requests: list[httpx.Request] = []

    def fake(request: httpx.Request) -> httpx.Response:
        requests.append(request)
        return httpx.Response(200)

    with pytest.MonkeyPatch.context() as baseline:
        baseline.setenv("SVE_DATA_DIR", str(outside))
        monkeypatch.setenv("SVE_DATA_DIR", str(tmp_path / "fake-data"))
        monkeypatch.setattr(
            cli,
            "http_factory",
            lambda _settings: httpx.AsyncClient(transport=httpx.MockTransport(fake)),
        )
        monkeypatch.undo()
        result = CliRunner().invoke(cli.app, ["crawl", "p0", "--max-requests", "1"])
        assert result.exit_code != 0
        assert isinstance(result.exception, IsolationError)
        assert "SVE_DATA_DIR" in str(result.exception)
        assert not outside.exists()
        assert requests == []


@pytest.mark.parametrize("missing", [False, True])
def test_unsafe_or_missing_environment_blocks_even_a_temporary_manifest(
    test_isolation_guard: IsolationGuard,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    missing: bool,
) -> None:
    if missing:
        monkeypatch.delenv("SVE_DATA_DIR")
    else:
        monkeypatch.setenv(
            "SVE_DATA_DIR", str(forbidden_root(test_isolation_guard, tmp_path))
        )
    path = tmp_path / "untouched/manifest.sqlite"
    with pytest.raises(IsolationError, match="SVE_DATA_DIR"):
        Manifest.open(path)
    assert not path.parent.exists()


@pytest.mark.parametrize("mode", ["write", "live", "snapshot", "sqlite-uri"])
def test_all_manifest_open_paths_outside_temp_are_rejected(
    test_isolation_guard: IsolationGuard,
    tmp_path: Path,
    mode: str,
) -> None:
    path = forbidden_root(test_isolation_guard, tmp_path) / "manifest.sqlite"

    def attempt() -> None:
        if mode == "write":
            Manifest.open(path)
        elif mode == "live":
            Manifest.open_live(path)
        elif mode == "snapshot":
            Manifest.open_snapshot(path)
        else:
            sqlite3.connect(f"{path.as_uri()}?mode=ro", uri=True)

    with pytest.raises(IsolationError, match="outside the pytest temporary root"):
        attempt()
    assert not path.exists()


def test_source_writer_outside_temp_is_rejected(
    test_isolation_guard: IsolationGuard,
    tmp_path: Path,
    manifest: Manifest,
) -> None:
    outside = forbidden_root(test_isolation_guard, tmp_path)
    writer = Writer(outside, manifest)
    request = manifest.requests.start(
        RequestStart("isolated", "fetch", 1, 0, URL, URL, None)
    )
    fetched = Fetched(
        URL,
        Region.JP,
        Kind.CARD,
        relpath("raw", "test.html"),
        b"synthetic",
        "text/html",
        None,
        None,
        False,
    )
    with pytest.raises(IsolationError):
        writer.write(fetched, request_id=request)
    assert not outside.exists()
    assert manifest.resources.get(URL) is None


def test_symlink_to_unapproved_data_does_not_authorize_writing(
    test_isolation_guard: IsolationGuard,
    tmp_path: Path,
) -> None:
    outside = forbidden_root(test_isolation_guard, tmp_path)
    alias = tmp_path / "alias"
    alias.symlink_to(outside)
    with pytest.raises(IsolationError):
        Manifest.open(alias / "manifest.sqlite")
    assert not outside.exists()
    alias.unlink()


def test_source_writer_checks_environment_at_write_time(
    tmp_path: Path,
    manifest: Manifest,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    writer = Writer(tmp_path / "data", manifest)
    request = manifest.requests.start(
        RequestStart("isolated", "fetch", 1, 0, URL, URL, None)
    )
    monkeypatch.delenv("SVE_DATA_DIR")
    fetched = Fetched(
        URL,
        Region.JP,
        Kind.CARD,
        relpath("raw", "test.html"),
        b"synthetic",
        "text/html",
        None,
        None,
        False,
    )
    with pytest.raises(IsolationError, match="SVE_DATA_DIR"):
        writer.write(fetched, request_id=request)
    assert not (tmp_path / "data").exists()


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


def test_metadata_only_source_write_cannot_bypass_environment_guard(
    tmp_path: Path,
    manifest: Manifest,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    writer = Writer(tmp_path / "data", manifest)
    fetched = Fetched(
        URL,
        Region.JP,
        Kind.CARD,
        relpath("raw", "test.html"),
        b"synthetic",
        "text/html",
        None,
        None,
        False,
    )
    first = manifest.requests.start(
        RequestStart("isolated", "first", 1, 0, URL, URL, None)
    )
    writer.write(fetched, request_id=first)
    before = manifest.resources.get(URL)
    second = manifest.requests.start(
        RequestStart("isolated", "second", 1, 0, URL, URL, None)
    )
    monkeypatch.delenv("SVE_DATA_DIR")
    with pytest.raises(IsolationError, match="SVE_DATA_DIR"):
        writer.write(fetched, request_id=second)
    assert manifest.resources.get(URL) == before


def test_in_memory_manifest_still_requires_explicit_test_environment(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.delenv("SVE_DATA_DIR")
    with pytest.raises(IsolationError, match="SVE_DATA_DIR"):
        Manifest.open_empty()


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


@pytest.mark.parametrize("method", ["sendto", "sendmsg", "connected-sendmsg"])
def test_real_udp_loopback_delivery_is_allowed(method: str) -> None:
    with (
        socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as receiver,
        socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sender,
    ):
        receiver.bind(("127.0.0.1", 0))
        receiver.settimeout(1)
        address = receiver.getsockname()
        if method == "sendto":
            sent = sender.sendto(b"synthetic", address)
        elif method == "sendmsg":
            sent = sender.sendmsg([b"synthetic"], [], 0, address)
        else:
            sender.connect(address)
            sent = sender.sendmsg([b"synthetic"])
        assert sent == len(b"synthetic")
        assert receiver.recv(32) == b"synthetic"


@pytest.mark.parametrize("method", ["sendto", "sendmsg"])
def test_real_udp_non_loopback_is_blocked(method: str) -> None:
    with (
        socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as receiver,
        socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sender,
    ):
        receiver.bind(("127.0.0.1", 0))
        # Linux routes this wildcard destination locally even if the guard regresses.
        address = ("0.0.0.0", receiver.getsockname()[1])  # ruff: ignore[hardcoded-bind-all-interfaces] -- destination only, routed locally on Linux

        def attempt() -> None:
            if method == "sendto":
                sender.sendto(b"synthetic", address)
            else:
                sender.sendmsg([b"synthetic"], [], 0, address)

        with pytest.raises(IsolationError, match="external socket"):
            attempt()


def test_unix_stream_socket_in_temporary_root_is_allowed(tmp_path: Path) -> None:
    address = str(tmp_path / "stream.sock")
    with (
        socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as listener,
        socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client,
    ):
        listener.bind(address)
        listener.listen(1)
        listener.settimeout(1)
        client.connect(address)
        with listener.accept()[0] as peer:
            peer.settimeout(1)
            client.sendmsg([b"synthetic"])
            assert peer.recv(32) == b"synthetic"


def test_unix_datagram_socket_in_temporary_root_is_allowed(tmp_path: Path) -> None:
    address = str(tmp_path / "datagram.sock")
    with (
        socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM) as receiver,
        socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM) as sender,
    ):
        receiver.bind(address)
        receiver.settimeout(1)
        sender.sendto(b"sendto", address)
        assert receiver.recv(32) == b"sendto"
        sender.sendmsg([b"sendmsg"], [], 0, address)
        assert receiver.recv(32) == b"sendmsg"


@pytest.mark.parametrize("operation", ["bind", "connect", "sendto", "sendmsg"])
def test_unix_socket_outside_temporary_root_is_blocked(
    test_isolation_guard: IsolationGuard, tmp_path: Path, operation: str
) -> None:
    address = str(forbidden_root(test_isolation_guard, tmp_path) / "outside.sock")
    with socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM) as connection:

        def attempt() -> None:
            if operation == "bind":
                connection.bind(address)
            elif operation == "connect":
                connection.connect(address)
            elif operation == "sendto":
                connection.sendto(b"synthetic", address)
            else:
                connection.sendmsg([b"synthetic"], [], 0, address)

        with pytest.raises(IsolationError, match="outside the pytest temporary root"):
            attempt()


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


def test_fd_relative_removal_is_resolved_under_its_directory(tmp_path: Path) -> None:
    path = tmp_path / "relative"
    path.write_bytes(b"synthetic")
    fd = os.open(tmp_path, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.unlink(path.name, dir_fd=fd)
    finally:
        os.close(fd)
    assert not path.exists()


def test_localhost_real_transport_is_allowed(localhost_server: str) -> None:
    with httpx.Client(trust_env=False) as client:
        assert client.get(localhost_server).content == b"localhost synthetic server"


async def test_async_localhost_real_transport_is_allowed(localhost_server: str) -> None:
    async with httpx.AsyncClient(trust_env=False) as client:
        assert (
            await client.get(localhost_server)
        ).content == b"localhost synthetic server"
