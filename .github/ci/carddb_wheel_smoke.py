"""Exercise installed wheel resources and the public reader on synthetic fixtures."""

import sys
from importlib.resources import files
from pathlib import Path

from sve_carddb.contracts.contract import schema
from sve_carddb.contracts.generate_schema import generate
from sve_carddb.core.json import array, canonical, object_value, parse, string
from sve_carddb.snapshot.read_api import current_image_keys, read_index
from sve_carddb.snapshot.reader import read_snapshot


def main() -> None:
    """Run under the isolated wheel interpreter, without the checkout on sys.path."""
    fixture = Path(sys.argv[1]).resolve()
    resource = files("sve_carddb.contracts").joinpath("schema/v2/contract.schema.json")
    assert "site-packages" in str(resource)
    assert schema()["$id"] == "urn:sve-kit:snapshot:2.0.0"
    assert generate() == resource.read_bytes()
    assert not any(
        name == prefix or name.startswith(prefix + ".")
        for name in sys.modules
        for prefix in (
            "sve_carddb.build_db",
            "sve_carddb.snapshot.export",
            "sve_carddb.snapshot.preview",
            "sve_carddb.snapshot.project",
            "sve_carddb.r2_upload",
        )
    )
    manifest = object_value(parse((fixture / "manifest.json").read_bytes()))
    payloads = {
        string(row["key"]): canonical(
            parse((fixture / "payloads" / Path(string(row["path"])).name).read_bytes())
        )
        for raw in array(manifest["files"])
        for row in (object_value(raw),)
    }
    assert read_snapshot(manifest, payloads)
    current_image_keys(manifest, payloads)
    index_case = next(
        object_value(case)["value"]
        for case in array(parse((fixture / "schema-valid.json").read_bytes()))
        if object_value(case)["schema"] == "Index"
    )
    index = read_index(canonical(index_case))
    index_format = 2
    assert index["index_format"] == index_format
    sys.stdout.write("Installed wheel schemas and public reader passed\n")


if __name__ == "__main__":
    main()
