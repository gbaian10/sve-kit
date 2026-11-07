"""Release folder for the publish step: models and index files under their content hash, plus manifest.json
(docs/schema/scan/artifacts.md). Upload is a separate, approved step; nothing here talks to the network.

usage: python -m sve_scan release <out dir> <det version> <det tag> <rec version> <rec tag>
  det model: SVE_SCAN_WORK/det_<det tag>.w8.onnx; rec model: SVE_SCAN_WORK/<rec tag>.w8.onnx;
  indexes: SVE_SCAN_WORK/index/<rec tag>/{jp,en}.*; data_version from SVE_SCAN_WORK/gallery-meta.json
"""

import json
import sys
from pathlib import Path

from sve_scan import artifacts, paths, runtime


def main() -> None:
    out, det_v, det_tag, rec_v, rec_tag = (
        Path(sys.argv[1]),
        int(sys.argv[2]),
        sys.argv[3],
        int(sys.argv[4]),
        sys.argv[5],
    )
    work = paths.work()
    data_version = json.loads((work / "gallery-meta.json").read_text())["data_version"]
    folder = runtime.index_dir(rec_tag)
    indexes = {ed: (folder, ed, data_version) for ed in artifacts.EDITIONS if (folder / f"{ed}.i8").exists()}
    manifest = artifacts.release(
        out, (det_v, work / f"det_{det_tag}.w8.onnx"), (rec_v, work / f"{rec_tag}.w8.onnx"), indexes
    )
    print(json.dumps(manifest, indent=1))


if __name__ == "__main__":
    main()
