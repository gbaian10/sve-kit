import json
from pathlib import Path

import numpy as np
import pytest

from sve_scan import artifacts


def card(i: int) -> dict:
    return {
        "no": f"T01-{i:03d}",
        "c": f"c:{i}",
        "n": 1000 + i,
        "f": 0,
        "nm": f"name {i}",
        "a": f"a:{i}",
        "k": "elf",
        "y": "follower",
    }


def index(n: int = 5, seed: int = 0) -> artifacts.Index:
    rng = np.random.default_rng(seed)
    vectors, scale = artifacts.quantize(rng.standard_normal((n, artifacts.DIM)).astype(np.float32))
    return artifacts.Index(vectors, scale, rng.random(n).astype(np.float32), [card(i) for i in range(n)])


def test_quantize_keeps_unit_rows_close():
    emb = np.random.default_rng(1).standard_normal((50, artifacts.DIM)).astype(np.float32)
    unit = emb / np.linalg.norm(emb, axis=1, keepdims=True)
    vectors, scale = artifacts.quantize(emb)
    assert vectors.dtype == np.int8 and scale.dtype == np.float32
    assert np.abs(vectors).max(axis=1).tolist() == [127] * 50  # the largest component uses the full range
    back = vectors.astype(np.float32) * scale[:, None]
    assert np.abs(back - unit).max() <= scale.max() / 2 + 1e-7


def test_index_round_trip(tmp_path: Path):
    idx = index()
    files = artifacts.write_index(tmp_path, "jp", idx)
    assert {p.name for p in files.values()} == {"jp.i8", "jp.scale", "jp.hub", "jp.json"}
    assert files["vectors"].stat().st_size == 5 * artifacts.DIM
    assert files["scale"].stat().st_size == 5 * 4
    back = artifacts.read_index(tmp_path, "jp")
    assert np.array_equal(back.vectors, idx.vectors)
    assert np.array_equal(back.scale, idx.scale) and np.array_equal(back.hub, idx.hub)
    assert back.cards == idx.cards


def test_scores_match_the_client_formula():
    idx = index()
    q = idx.vectors[2].astype(np.float32) * idx.scale[2]
    q /= np.linalg.norm(q)
    got = artifacts.scores(idx, q, hub_weight=0.75)
    want = [float(idx.vectors[i].astype(np.float32) @ q * idx.scale[i] - 0.75 * idx.hub[i]) for i in range(5)]
    assert np.allclose(got, want, atol=1e-6)


@pytest.mark.parametrize(
    "change",
    [
        {"vectors": np.zeros((5, artifacts.DIM), np.float32)},
        {"scale": np.zeros(4, np.float32)},
        {"hub": np.zeros(5, np.float64)},
    ],
)
def test_index_rejects_wrong_arrays(change):
    good = index()
    parts = {"vectors": good.vectors, "scale": good.scale, "hub": good.hub, "cards": good.cards} | change
    with pytest.raises(ValueError):
        artifacts.Index(**parts)


def test_index_rejects_other_card_fields():
    good = index()
    cards = [dict(c, extra=1) for c in good.cards]
    with pytest.raises(ValueError, match="card fields"):
        artifacts.Index(good.vectors, good.scale, good.hub, cards)


def test_read_index_rejects_a_short_vector_file(tmp_path: Path):
    artifacts.write_index(tmp_path, "en", index())
    (tmp_path / "en.i8").write_bytes(b"\0" * 10)
    with pytest.raises(ValueError, match="expected 5"):
        artifacts.read_index(tmp_path, "en")


def test_release_hashes_every_file_and_writes_the_manifest(tmp_path: Path):
    work = tmp_path / "work"
    work.mkdir()
    det, rec = work / "det.onnx", work / "rec.onnx"
    det.write_bytes(b"det model")
    rec.write_bytes(b"rec model")
    artifacts.write_index(work / "idx", "jp", index(4))
    artifacts.write_index(work / "idx", "en", index(3, seed=2))
    out = tmp_path / "release"
    manifest = artifacts.release(out, (1, det), (2, rec), {ed: (work / "idx", ed, "2026-10-29") for ed in ("jp", "en")})
    assert manifest == json.loads((out / "manifest.json").read_text())
    assert manifest["format"] == artifacts.FORMAT
    assert manifest["det"]["version"] == 1 and manifest["det"]["grow"] == artifacts.DET_PARAMS["grow"]
    assert manifest["rec"]["version"] == 2 and manifest["rec"]["scoring"] == artifacts.REC_SCORING
    assert manifest["index"]["jp"]["count"] == 4 and manifest["index"]["en"]["count"] == 3
    assert manifest["index"]["jp"]["rec"] == 2 and manifest["index"]["jp"]["data_version"] == "2026-10-29"
    entries = [manifest["det"], manifest["rec"]] + [
        manifest["index"][ed][part] for ed in ("jp", "en") for part in artifacts.SUFFIX
    ]
    for entry in entries:
        path = out / entry["file"]
        assert path.name.startswith(entry["sha256"])  # content-addressed, so it can be cached forever
        assert artifacts.sha256(path) == entry["sha256"] and path.stat().st_size == entry["bytes"]
    assert manifest["det"]["file"].startswith("models/") and manifest["index"]["en"]["hub"]["file"].startswith("index/")


def test_release_refuses_unknown_editions(tmp_path: Path):
    artifacts.write_index(tmp_path / "idx", "jp", index())
    (tmp_path / "m.onnx").write_bytes(b"m")
    with pytest.raises(ValueError, match="unknown editions"):
        artifacts.release(
            tmp_path / "out", (1, tmp_path / "m.onnx"), (1, tmp_path / "m.onnx"), {"cn": (tmp_path / "idx", "jp", "x")}
        )
