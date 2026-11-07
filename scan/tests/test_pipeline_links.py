"""Steps hand files to each other: these pin the links the review found broken (no models, no private data)."""

import json
from email.message import Message
from pathlib import Path

import numpy as np
import pytest

from sve_scan import artifacts, deck_label, fetch, rec_index, release
from sve_scan import gallery as GL


def rows(*specs: tuple[int, int, str]) -> list[dict]:
    return [{"n": n, "f": f, "card_version": v} for n, f, v in specs]


# ---------- gallery: the image array must belong to these rows ----------
def test_gallery_identity_changes_with_new_cards_versions_and_order():
    base = rows((20001, 0, "3"), (20002, 0, "1"))
    same = GL.rows_sha256(base)
    assert GL.rows_sha256(rows((20001, 0, "3"), (20002, 0, "1"))) == same
    assert GL.rows_sha256(base + rows((20003, 0, "1"))) != same  # a new card
    assert GL.rows_sha256(rows((20001, 0, "4"), (20002, 0, "1"))) != same  # same count, new image version
    assert GL.rows_sha256(rows((20002, 0, "1"), (20001, 0, "3"))) != same  # same rows, another order
    assert GL.rows_sha256(rows((20001, 1, "3"), (20002, 0, "1"))) != same  # back face instead of front


def test_steps_refuse_an_image_array_built_for_other_rows(tmp_path: Path):
    gallery = rows((20001, 0, "3"))
    (tmp_path / "gallery.json").write_text(json.dumps(gallery))
    (tmp_path / "gallery-meta.json").write_text(json.dumps({"rows_sha256": GL.rows_sha256(gallery)}))
    assert GL.current(tmp_path)[0] == gallery
    (tmp_path / "gallery.json").write_text(json.dumps(gallery + rows((20002, 0, "1"))))
    with pytest.raises(SystemExit, match="rerun the gallery step"):
        GL.current(tmp_path)


# ---------- deck labels: one file name for the writer and every reader ----------
@pytest.mark.parametrize(("i", "name"), [(1, "001.json"), (42, "042.json"), (123, "123.json"), ("7", "007.json")])
def test_label_file_names_are_zero_padded(i, name):
    assert deck_label.label_name(i) == name


# ---------- corner finder output ----------
def test_decode_quad_reads_offsets_in_cells_from_the_cell_centre():
    corners = np.zeros((8, 32, 32), np.float32)
    corners[:, 5, 7] = [-1, -1, 1, -1, 1, 1, -1, 1]  # a 2x2-cell square around cell (7, 5)
    quad = artifacts.decode_quad(corners, gx=7, gy=5)
    assert np.allclose(quad, [[52, 36], [68, 36], [68, 52], [52, 52]])  # (7.5 +- 1) * 8, (5.5 +- 1) * 8


@pytest.mark.parametrize("shift", range(4))
def test_corners_from_any_start_come_out_top_left_clockwise(shift):
    upright = np.float32([[10, 20], [60, 22], [58, 90], [8, 88]])  # TL, TR, BR, BL of a slightly tilted card
    got = artifacts.order_corners(np.roll(upright, shift, axis=0))
    assert np.array_equal(got, upright)
    assert np.array_equal(artifacts.order_corners(upright[::-1].copy()), upright)  # even counter-clockwise input


def test_a_card_seen_upside_down_still_starts_top_left():
    turned = np.float32([[58, 90], [8, 88], [10, 20], [60, 22]])  # the model's start corner is bottom right
    assert artifacts.order_corners(turned)[0].tolist() == [10, 20]


# ---------- release: the index's own card data version ----------
def make_work(tmp_path: Path, data_version: str) -> Path:
    work = tmp_path / "work"
    (work / "index" / "rec").mkdir(parents=True)
    (work / "det_det.w8.onnx").write_bytes(b"det")
    (work / "rec.w8.onnx").write_bytes(b"rec")
    rng = np.random.default_rng(0)
    vec, scale = artifacts.quantize(rng.standard_normal((2, artifacts.DIM)).astype(np.float32))
    cards = [{"no": f"T-{i}", "c": f"c:{i}", "n": i, "f": 0, "nm": "", "a": "", "k": "", "y": ""} for i in range(2)]
    artifacts.write_index(work / "index" / "rec", "jp", artifacts.Index(vec, scale, np.zeros(2, np.float32), cards))
    build = {"data_version": data_version, "rec_sha256": artifacts.sha256(work / "rec.w8.onnx"), "editions": ["jp"]}
    (work / "index" / "rec" / "build.json").write_text(json.dumps(build))
    return work


def test_release_labels_an_index_with_the_card_data_it_was_built_from(tmp_path: Path):
    work = make_work(tmp_path, "A")
    (work / "gallery-meta.json").write_text(json.dumps({"data_version": "B"}))  # a newer gallery, index not rebuilt
    manifest = release.run(tmp_path / "out", 1, "det", 1, "rec", work)
    assert manifest["index"]["jp"]["data_version"] == "A"


def test_release_refuses_an_index_built_with_another_model(tmp_path: Path):
    work = make_work(tmp_path, "A")
    (work / "rec.w8.onnx").write_bytes(b"retrained rec")
    with pytest.raises(SystemExit, match="rerun the index step"):
        release.run(tmp_path / "out", 1, "det", 1, "rec", work)


# ---------- downloads: 2 s apart, every file described, reruns keep the record ----------
class FakeWeb:
    def __init__(self) -> None:
        self.now, self.requests, self.waits = 100.0, [], []

    def clock(self) -> float:
        return self.now

    def sleep(self, s: float) -> None:
        self.waits.append(s)
        self.now += s

    def open(self, url: str) -> tuple[bytes, Message]:
        self.requests.append((url, self.now))
        headers = Message()
        headers["ETag"] = '"abc"'
        return (b"[]" if "api" in url else b"image bytes"), headers


def test_requests_are_at_least_two_seconds_apart(tmp_path: Path):
    web = FakeWeb()
    polite = fetch.Polite(tmp_path, clock=web.clock, sleep=web.sleep, opener=web.open)
    polite.json("https://api.example/list")
    web.now += 0.5  # the caller works for a moment
    polite.download("https://files.example/a.jpg", "a.jpg")
    polite.json("https://api.example/next")
    times = [t for _, t in web.requests]
    assert all(b - a >= fetch.MIN_GAP for a, b in zip(times, times[1:]))


def test_every_download_keeps_url_time_etag_and_hash_across_reruns(tmp_path: Path):
    web = FakeWeb()
    first = fetch.Polite(tmp_path, clock=web.clock, sleep=web.sleep, opener=web.open)
    rec = first.download("https://files.example/a.jpg", "a.jpg", license="CC0")
    first.save()
    assert rec["url"] == "https://files.example/a.jpg" and rec["etag"] == '"abc"' and rec["fetched_at"]
    assert rec["sha256"] == artifacts.sha256(tmp_path / "a.jpg") and rec["bytes"] == len(b"image bytes")
    again = fetch.Polite(tmp_path, clock=web.clock, sleep=web.sleep, opener=web.open)
    n = len(web.requests)
    assert again.download("https://files.example/a.jpg", "a.jpg", license="CC0") == rec
    again.save()
    assert len(web.requests) == n  # nothing fetched twice
    assert json.loads((tmp_path / "manifest.json").read_text()) == [rec]  # the first record survives the rerun


def test_a_file_from_before_records_gets_its_hash_and_an_honest_unknown_time(tmp_path: Path):
    (tmp_path / "old.jpg").write_bytes(b"old")
    web = FakeWeb()
    polite = fetch.Polite(tmp_path, clock=web.clock, sleep=web.sleep, opener=web.open)
    rec = polite.download("https://x/old.jpg", "old.jpg")
    assert web.requests == [] and rec["fetched_at"] is None and rec["etag"] is None
    assert rec["sha256"] == artifacts.sha256(tmp_path / "old.jpg")


def test_a_failed_run_keeps_the_records_of_files_it_already_downloaded(tmp_path: Path):
    web = FakeWeb()
    real_open = web.open

    def flaky(url: str) -> tuple[bytes, Message]:
        if "second" in url:
            raise OSError("network down")
        return real_open(url)

    first = fetch.Polite(tmp_path, clock=web.clock, sleep=web.sleep, opener=flaky)
    rec = first.download("https://files.example/first.jpg", "first.jpg")
    with pytest.raises(OSError):
        first.download("https://files.example/second.jpg", "second.jpg")
    # no save() call: the run died; a new run must still know where first.jpg came from and when
    again = fetch.Polite(tmp_path, clock=web.clock, sleep=web.sleep, opener=web.open)
    assert again.complete("first.jpg") and again.records["first.jpg"] == rec and rec["etag"] == '"abc"'
    assert not (tmp_path / "second.jpg").exists() and not again.complete("second.jpg")


def test_a_record_without_its_url_is_not_complete_and_gets_the_url_back(tmp_path: Path):
    (tmp_path / "room.hdr").write_bytes(b"hdr")
    (tmp_path / "manifest.json").write_text(json.dumps([{"file": "room.hdr", "id": "room", "license": "CC0"}]))
    web = FakeWeb()
    polite = fetch.Polite(tmp_path, clock=web.clock, sleep=web.sleep, opener=web.open)
    assert not polite.complete("room.hdr")
    rec = polite.download("https://files.example/room.hdr", "room.hdr", id="room")
    assert rec["url"] == "https://files.example/room.hdr" and rec["fetched_at"] is None
    assert rec["sha256"] == artifacts.sha256(tmp_path / "room.hdr") and web.requests == []


# ---------- index rebuild: nothing from an earlier build survives it ----------
class FakeRecogniser:
    def embed(self, arr: np.ndarray, bs: int = 64) -> np.ndarray:
        return np.random.default_rng(len(arr)).standard_normal((len(arr), artifacts.DIM)).astype(np.float32)


def write_gallery(work: Path, regions: list[str], data_version: str) -> None:
    meta = [
        {
            "n": 20000 + i,
            "f": 0,
            "card_version": "1",
            "card_no": f"T-{i}",
            "card_id": f"c:{i}",
            "name": "",
            "art_id": None,
            "class": "",
            "type": "",
            "region": r,
        }
        for i, r in enumerate(regions)
    ]
    (work / "gallery.json").write_text(json.dumps(meta))
    np.save(work / "gallery.npy", np.zeros((len(meta), 4, 4, 3), np.uint8))
    rows = GL.rows_sha256(meta)
    (work / "gallery-meta.json").write_text(
        json.dumps({"data_version": data_version, "manifest_sha256": "m", "rows_sha256": rows})
    )
    np.save(work / "hub_r_rec.npy", np.zeros(len(meta), np.float32))
    (work / "hub_r_rec.json").write_text(json.dumps({"rows_sha256": rows}))


def test_rebuilding_with_fewer_editions_releases_only_what_was_rebuilt(tmp_path: Path):
    work = tmp_path / "work"
    work.mkdir()
    (work / "rec.w8.onnx").write_bytes(b"rec")
    (work / "det_det.w8.onnx").write_bytes(b"det")
    write_gallery(work, ["jp", "en", "jp"], "A")
    assert rec_index.build(work, "rec", FakeRecogniser())["editions"] == ["jp", "en"]
    write_gallery(work, ["jp", "jp"], "B")  # a jp-only export
    assert rec_index.build(work, "rec", FakeRecogniser())["editions"] == ["jp"]
    assert sorted(p.name for p in (work / "index" / "rec").iterdir()) == [
        "build.json",
        "jp.hub",
        "jp.i8",
        "jp.json",
        "jp.scale",
    ]
    manifest = release.run(tmp_path / "out", 1, "det", 1, "rec", work)
    assert list(manifest["index"]) == ["jp"] and manifest["index"]["jp"]["data_version"] == "B"
