"""Per-run GC approval, shared lease and fail-closed index/scope checks."""

import json
from copy import deepcopy
from typing import TYPE_CHECKING

import httpx
import pytest
from typer.testing import CliRunner

from sve_carddb.cli import app
from sve_carddb.r2_upload.v2 import gc
from sve_carddb.r2_upload.v2.adapter import LEASE_HEADERS, LEASE_KEY, R2Store
from sve_carddb.r2_upload.v2.bundle import save_checkpoint
from sve_carddb.snapshot.publish import publish
from sve_carddb.snapshot.publish.plan import INDEX
from sve_carddb.snapshot.publish.storage import PublishError, Stored
from sve_carddb.snapshot.values import canonical, digest, string

from .r2_v2_fixtures import ACCOUNT, BUCKET
from .r2_v2_fixtures import remote as remote  # ruff: ignore[useless-import-alias] -- shared loopback server fixture
from .r2_v2_fixtures import server as server  # ruff: ignore[useless-import-alias] -- dependency of remote
from .snapshot_publish_fixtures import FakeCDN, FakeS3, candidate, version
from .snapshot_publish_fixtures import images as images  # ruff: ignore[useless-import-alias] -- dependency of frozen_base
from .snapshot_publish_fixtures import ledger as ledger  # ruff: ignore[useless-import-alias] -- independent two-version history
from .test_r2_v2_bundle import frozen as frozen  # ruff: ignore[useless-import-alias] -- isolated state copies
from .test_r2_v2_bundle import frozen_base as frozen_base  # ruff: ignore[useless-import-alias] -- shared synthetic producer

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue

    from sve_carddb.snapshot.publish import Ledger

    from .r2_v2_fixtures import Loopback, ServerState
    from .test_r2_v2_bundle import Frozen
    from .test_snapshot_preview_images import PublicImages

ORPHAN = "snapshots/blobs/" + "f" * 64 + ".json"
NAMESPACES = frozenset({"snapshots/blobs/", "snapshots/manifests/", "images/card_s/"})


@pytest.fixture(scope="module")
def published_base(frozen_base: Frozen) -> dict[str, Stored]:
    store = FakeS3()
    publish(frozen_base.ledger, store, frozen_base.release, FakeCDN(store))
    save_checkpoint(frozen_base.checkpoint, frozen_base.ledger)
    return deepcopy(store.objects)


@pytest.fixture
def published(
    published_base: dict[str, Stored],
    frozen: Frozen,
    remote: tuple[R2Store, ServerState, Loopback],
) -> tuple[Frozen, R2Store, ServerState]:
    store, state, _ = remote
    state.objects = deepcopy(published_base)
    state.objects[ORPHAN] = Stored(
        b"orphan", '"orphan"', {"content-type": "application/json"}
    )
    state.objects["raw/never-delete"] = Stored(b"protected", '"raw"', {})
    return frozen, store, state


def test_remote_inspection_only_lists_orphans_then_exact_confirmation_deletes(
    published: tuple[Frozen, R2Store, ServerState],
) -> None:
    frozen, store, state = published
    before = deepcopy(state.objects)
    plan = gc.inspect(frozen.ledger, store, namespaces=NAMESPACES)
    assert state.objects[ORPHAN] == before[ORPHAN]
    assert gc.report(plan)["would_collect"] == [ORPHAN]
    assert gc.report(plan)["candidate_bytes"] == 6
    assert not any(op[0] == "DELETE" for op in state.operations)
    deleted = gc.execute(frozen.ledger, store, plan, confirmed=gc.confirmation(plan))
    assert deleted == (ORPHAN,)
    assert ORPHAN not in state.objects
    assert state.objects["raw/never-delete"] == before["raw/never-delete"]
    assert state.objects[INDEX] == before[INDEX]
    assert [op for op in state.operations if op[0] == "DELETE"] == [
        ("DELETE", ORPHAN, None)
    ]
    delete = next(i for i, r in enumerate(state.requests) if r[0] == "DELETE")
    assert "if-match" not in state.requests[delete][2]
    assert state.requests[delete - 2][1].endswith("/" + INDEX)
    assert state.requests[delete - 1][1].endswith("/" + LEASE_KEY)


def test_wrong_confirmation_is_rejected_before_any_remote_request(
    published: tuple[Frozen, R2Store, ServerState],
) -> None:
    frozen, store, state = published
    plan = gc.inspect(frozen.ledger, store, namespaces=NAMESPACES)
    count = len(state.requests)
    with pytest.raises(
        PublishError, match=r"^GC requires the exact contemporary confirmation string$"
    ):
        gc.execute(frozen.ledger, store, plan, confirmed="DELETE-V2 wrong")
    assert len(state.requests) == count
    assert ORPHAN in state.objects


@pytest.mark.parametrize("change", ["index", "lease", "candidate"])
def test_change_immediately_before_delete_preserves_every_candidate(
    published: tuple[Frozen, R2Store, ServerState],
    monkeypatch: pytest.MonkeyPatch,
    change: str,
) -> None:
    frozen, store, state = published
    plan = gc.inspect(frozen.ledger, store, namespaces=NAMESPACES)
    real = store.delete_approved

    def raced(key: str, *, index: Stored, candidate: dict[str, JsonValue]) -> None:
        if change == "index":
            previous = state.objects[INDEX]
            state.objects[INDEX] = Stored(
                previous.raw, '"changed-index"', previous.headers
            )
        elif change == "lease":
            state.objects[LEASE_KEY] = Stored(
                b"foreign owner", '"foreign"', LEASE_HEADERS
            )
        else:
            state.objects[key] = Stored(b"different candidate", '"different"', {})
        real(key, index=index, candidate=candidate)

    monkeypatch.setattr(store, "delete_approved", raced)
    with pytest.raises(
        PublishError,
        match=r"^GC current/previous index changed before deletion$|^Deployment writer lease changed; stop and review$|^GC candidate changed before deletion$",
    ):
        gc.execute(frozen.ledger, store, plan, confirmed=gc.confirmation(plan))
    assert ORPHAN in state.objects
    assert not any(op[0] == "DELETE" for op in state.operations)


@pytest.mark.parametrize(
    "scope", [frozenset({"raw/"}), frozenset({"images/"}), frozenset()]
)
def test_namespace_outside_the_explicit_allowlist_is_rejected(
    published: tuple[Frozen, R2Store, ServerState], scope: frozenset[str]
) -> None:
    frozen, store, state = published
    with pytest.raises(PublishError, match=r"^GC namespace is not explicitly public$"):
        gc.inspect(frozen.ledger, store, namespaces=scope)
    assert not any(op[0] == "DELETE" for op in state.operations)


def test_irregular_key_stops_the_whole_inventory_before_deletion(
    published: tuple[Frozen, R2Store, ServerState],
) -> None:
    frozen, store, state = published
    state.objects["snapshots/blobs/unrecognized.txt"] = Stored(b"unknown", '"x"', {})
    with pytest.raises(
        PublishError, match=r"^GC inventory contains an irregular public key$"
    ):
        gc.inspect(frozen.ledger, store, namespaces=NAMESPACES)
    assert ORPHAN in state.objects
    assert not any(op[0] == "DELETE" for op in state.operations)


def test_tampered_saved_list_and_new_inventory_require_a_new_confirmation(
    published: tuple[Frozen, R2Store, ServerState],
) -> None:
    frozen, store, state = published
    plan = gc.inspect(frozen.ledger, store, namespaces=NAMESPACES)
    state.objects["snapshots/blobs/" + "e" * 64 + ".json"] = Stored(
        b"new orphan", '"n"', {}
    )
    with pytest.raises(
        PublishError,
        match=r"^GC inventory or current/previous changed; approve a new list$",
    ):
        gc.execute(frozen.ledger, store, plan, confirmed=gc.confirmation(plan))
    assert ORPHAN in state.objects
    assert not any(op[0] == "DELETE" for op in state.operations)


def test_offline_gc_cli_lists_saved_plan_without_reading_credentials(
    published: tuple[Frozen, R2Store, ServerState],
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    frozen, store, _state = published
    plan = gc.inspect(frozen.ledger, store, namespaces=NAMESPACES)
    path = tmp_path / "gc.json"
    path.write_bytes(canonical(plan))

    def forbidden(**_kwargs: object) -> None:
        pytest.fail("offline GC opened a client")

    monkeypatch.setattr(httpx, "Client", forbidden)
    args = [
        "r2",
        "gc-v2",
        "--plan-file",
        str(path),
        "--ledger-dir",
        str(frozen.ledger.root),
        "--backup-dir",
        str(frozen.ledger.backup),
        "--checkpoint-file",
        str(frozen.checkpoint),
    ]
    result = CliRunner().invoke(app, args)
    assert result.exit_code == 0, result.output
    assert json.loads(result.output)["would_collect"] == [ORPHAN]
    denied = CliRunner().invoke(app, [*args, "--execute"])
    assert denied.exit_code == 2
    assert "GC execution requires authorization" in denied.output


def test_cli_inspection_then_explicit_consent_is_required_for_deletion(
    published: tuple[Frozen, R2Store, ServerState],
    server: tuple[ServerState, Loopback],
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    frozen, _store, state = published
    _state, transport = server
    real_client = httpx.Client

    def factory(**kwargs: object) -> httpx.Client:
        assert kwargs["trust_env"] is False
        assert kwargs["follow_redirects"] is False
        return real_client(transport=transport, trust_env=False, follow_redirects=False)

    monkeypatch.setattr(httpx, "Client", factory)
    monkeypatch.setenv("SVE_R2_ACCESS_KEY_ID", "synthetic-access")
    monkeypatch.setenv("SVE_R2_SECRET_ACCESS_KEY", "synthetic-secret")
    path = tmp_path / "approval.json"
    args = [
        "r2",
        "gc-v2",
        "--plan-file",
        str(path),
        "--ledger-dir",
        str(frozen.ledger.root),
        "--backup-dir",
        str(frozen.ledger.backup),
        "--checkpoint-file",
        str(frozen.checkpoint),
        "--account-id",
        ACCOUNT,
        "--bucket",
        BUCKET,
        "--confirm-maintainer-authorization",
    ]
    result = CliRunner().invoke(
        app, [*args, "--inspect-remote", "--namespace", "snapshots/blobs/"]
    )
    assert result.exit_code == 0, result.output
    consent = json.loads(result.output)["confirmation"]
    assert path.stat().st_mode & 0o777 == 0o600
    assert ORPHAN in state.objects
    assert not any(op[0] == "DELETE" for op in state.operations)
    denied = CliRunner().invoke(app, [*args, "--execute", "--confirm-delete", "wrong"])
    assert denied.exit_code == 2
    assert ORPHAN in state.objects
    executed = CliRunner().invoke(
        app, [*args, "--execute", "--confirm-delete", consent]
    )
    assert executed.exit_code == 0, executed.output
    assert json.loads(executed.output)["deleted"] == [ORPHAN]
    assert ORPHAN not in state.objects
    assert "synthetic-secret" not in executed.output


def test_previous_images_are_collectible_while_both_json_versions_stay(
    ledger: Ledger,
    images: PublicImages,
    remote: tuple[R2Store, ServerState, Loopback],
) -> None:
    store, state, _transport = remote
    origin = FakeS3()
    cdn = FakeCDN(origin)
    first = candidate(ledger, images)
    publish(ledger, origin, first, cdn)
    hidden = deepcopy(images.projection)
    hidden.tables["image_asset"][0].update(
        publication_state="withdrawn", withdrawal_reason="Synthetic withdrawal"
    )
    hidden.tables["image_variant"] = []
    second = candidate(ledger, images, projection=hidden, from_version=version(first))
    publish(ledger, origin, second, cdn)
    state.objects = deepcopy(origin.objects)
    before = deepcopy(state.objects)
    namespaces = NAMESPACES | frozenset(
        {"images/card_m/", "images/card_l/", "images/art_s/", "images/art_m/"}
    )
    expected = {string(a["path"]) for a in first.assets}
    assert expected
    plan = gc.inspect(ledger, store, namespaces=namespaces)
    assert gc.report(plan)["would_collect"] == sorted(expected)
    removed = gc.execute(ledger, store, plan, confirmed=gc.confirmation(plan))
    assert set(removed) == expected
    assert expected.isdisjoint(state.objects)
    assert all(state.objects[m.key] == before[m.key] for m in first.members)
    assert all(state.objects[m.key] == before[m.key] for m in second.members)
    assert all(
        op[1].startswith("images/") for op in state.operations if op[0] == "DELETE"
    )
    repeated = gc.inspect(ledger, store, namespaces=namespaces)
    assert gc.report(repeated)["would_collect"] == []
    assert (
        gc.execute(ledger, store, repeated, confirmed=gc.confirmation(repeated)) == ()
    )


def test_missing_retained_member_blocks_even_unrelated_candidates(
    published: tuple[Frozen, R2Store, ServerState],
) -> None:
    frozen, store, state = published
    del state.objects[frozen.release.members[0].key]
    with pytest.raises(PublishError):
        gc.inspect(frozen.ledger, store, namespaces=NAMESPACES)
    assert ORPHAN in state.objects
    assert not any(op[0] == "DELETE" for op in state.operations)


def test_delete_boundary_refuses_scope_outside_public_namespaces(
    published: tuple[Frozen, R2Store, ServerState],
) -> None:
    _frozen, store, state = published
    with (
        store.exclusive(),
        pytest.raises(
            PublishError,
            match=r"^GC deletion key is outside the approved public scope$",
        ),
    ):
        store.delete_approved(
            "raw/never-delete",
            index=state.objects[INDEX],
            candidate={
                "key": "raw/never-delete",
                "etag": '"raw"',
                "bytes": 9,
                "sha256": digest(b"protected"),
            },
        )
    assert "raw/never-delete" in state.objects
    assert not any(op[0] == "DELETE" for op in state.operations)


def test_lease_lost_after_final_index_read_still_refuses_delete(
    published: tuple[Frozen, R2Store, ServerState], monkeypatch: pytest.MonkeyPatch
) -> None:
    frozen, store, state = published
    plan = gc.inspect(frozen.ledger, store, namespaces=NAMESPACES)
    real_delete = store.delete_approved
    real_get = store.get

    def raced_get(key: str) -> Stored | None:
        value = real_get(key)
        if key == INDEX:
            state.objects[LEASE_KEY] = Stored(
                b"foreign owner", '"foreign"', LEASE_HEADERS
            )
        return value

    def raced_delete(
        key: str, *, index: Stored, candidate: dict[str, JsonValue]
    ) -> None:
        monkeypatch.setattr(store, "get", raced_get)
        real_delete(key, index=index, candidate=candidate)

    monkeypatch.setattr(store, "delete_approved", raced_delete)
    with pytest.raises(
        PublishError, match=r"^Deployment writer lease changed; stop and review$"
    ):
        gc.execute(frozen.ledger, store, plan, confirmed=gc.confirmation(plan))
    assert ORPHAN in state.objects
    assert not any(op[0] == "DELETE" for op in state.operations)


@pytest.mark.parametrize("status", [307, 403, 500])
def test_delete_failure_never_retries_or_prints_the_remote_body(
    published: tuple[Frozen, R2Store, ServerState], status: int
) -> None:
    frozen, store, state = published
    plan = gc.inspect(frozen.ledger, store, namespaces=NAMESPACES)
    state.delete_status = status
    with pytest.raises(
        PublishError, match=r"^R2 approved DELETE failed; inspect before retry$"
    ) as failure:
        gc.execute(frozen.ledger, store, plan, confirmed=gc.confirmation(plan))
    assert "synthetic-secret" not in str(failure.value)
    assert ORPHAN in state.objects
    assert [op for op in state.operations if op[0] == "DELETE"] == [
        ("DELETE", ORPHAN, None)
    ]
