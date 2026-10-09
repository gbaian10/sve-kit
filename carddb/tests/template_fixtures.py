"""Synthetic current-template packages for reader and validation tests."""

from sve_carddb.core.json import canonical
from sve_carddb.domains.translations.templates.files import Files
from sve_carddb.domains.translations.templates.records import Record, Shard


def template_files(files: Files, records: tuple[Record, ...]) -> Files:
    preserved = tuple(
        item
        for item in files.content
        if not item[0].startswith("translations/templates/")
    )
    content = list(preserved)
    for area, kinds in (
        ("definitions", {"sentence_template"}),
        ("values", {"template_translation", "template_translation_variant"}),
        ("candidates", {"template_translation_candidate"}),
    ):
        selected = tuple(record for record in records if record.kind in kinds)
        if not selected:
            continue
        shard = Shard(format=2, kind="translation_shard", records=selected)
        raw = canonical(shard.model_dump(mode="json", round_trip=True))
        content.append((f"translations/templates/{area}/001.yaml", raw, raw))
    return Files(files.revision, tuple(sorted(content)))
