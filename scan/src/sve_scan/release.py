"""Release folder for the publish step: models and index files under their content hash, plus manifest.json
(docs/schema/scan/artifacts.md). Upload is a separate, approved step; nothing here talks to the network.

usage: python -m sve_scan release <out dir> <det version> <det tag> <rec version> <rec tag>
  det model: SVE_SCAN_WORK/det_<det tag>.w8.onnx; rec model: SVE_SCAN_WORK/<rec tag>.w8.onnx;
  indexes: SVE_SCAN_WORK/index/<rec tag>/{jp,en}.*; their card data version from that folder's build.json
"""

import json
import sys
from pathlib import Path

from sve_scan import artifacts, paths


def run(out: Path, det_v: int, det_tag: str, rec_v: int, rec_tag: str, work: Path) -> artifacts.Json:
    """Release folder from the work folder; the index's card data version comes from its own build record."""
    folder = work / "index" / rec_tag
    build = json.loads((folder / "build.json").read_text())
    rec_path = work / f"{rec_tag}.w8.onnx"
    if build["rec_sha256"] != artifacts.sha256(rec_path):
        raise SystemExit(f"the index in {folder} was built with another {rec_path.name}: rerun the index step")
    # only the editions this build made: files lying in the folder are not evidence of anything
    indexes = {ed: (folder, ed, build["data_version"]) for ed in build["editions"]}
    return artifacts.release(out, (det_v, work / f"det_{det_tag}.w8.onnx"), (rec_v, rec_path), indexes)


def main() -> None:
    out, det_v, det_tag, rec_v, rec_tag = sys.argv[1:6]
    print(json.dumps(run(Path(out), int(det_v), det_tag, int(rec_v), rec_tag, paths.work()), indent=1))


if __name__ == "__main__":
    main()
