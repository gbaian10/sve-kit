"""Bind optional regional images to the same sealed offline build transaction."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build_inputs import input_record, uses_sorted
from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.image_assets import (
    plan_regional_images,
    populate_assets,
    reference_uses,
    verify_asset_sources,
    verify_assets,
)
from sve_carddb.image_crop_report import crop_report
from sve_carddb.image_crops import load_image_crops

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build_db import Database
    from sve_carddb.build_inputs import BuildContext, InputRecord
    from sve_carddb.image_assets import ImageBuild
    from sve_carddb.image_crops import ImageCrops
    from sve_carddb.registry.preview import PreviewPlan
    from sve_carddb.snapshot.offline import Inputs


@dataclass(frozen=True)
class MountedImages:
    assets: ImageBuild
    root: Path
    crops: ImageCrops

    def populate(
        self,
        db: Database,
        inputs: Inputs,
        identity: PreviewPlan,
        context: BuildContext,
        parents: InputRecord,
    ) -> tuple[InputRecord, dict[str, JsonValue]]:
        """Replay exact page bindings in both staging and sealed bundle databases."""
        self.crops.verify_context(context)
        references = tuple(
            ref
            for pin in inputs.sources
            for ref in plan_regional_images(
                db,
                identity,
                FrozenSources(inputs.archive, inputs.store_id, pin.card_batch),
                region=pin.region,
            )
        )
        added = populate_assets(db, self.assets, references, self.root)
        record = input_record(context, (*parents.uses, *added))
        expected = uses_sorted(
            (*parents.uses, *self.assets.source_uses(), *reference_uses(references))
        )
        record.verify(db, context, expected)
        report = {
            key: value
            for key, value in self.assets.report(references).items()
            if key not in {"elapsed_milliseconds", "cache_hits"}
        }
        report["crop_overrides"] = crop_report(self.crops, self.assets, references, db)
        return record, report


def prepare_images(
    inputs: Inputs, assets: ImageBuild | None, root: Path | None
) -> MountedImages | None:
    """Require complete current membership in both explicit regional image pins."""
    if (assets is None) != (root is None):
        raise ValueError("Image build and asset root must be provided together")
    if assets is None or root is None:
        return None
    if not root.is_absolute() or root.is_symlink():
        raise ValueError("Image asset root must be absolute and not a symlink")
    for protected in (inputs.repo, inputs.archive):
        if root.resolve().is_relative_to(
            protected.resolve()
        ) or protected.resolve().is_relative_to(root.resolve()):
            raise ValueError("Image asset root overlaps protected offline inputs")
    crops = load_image_crops(
        inputs.repo / "authored", authored_revision=inputs.revision
    )
    pins = {pin.region: pin.image_batch for pin in inputs.sources}
    if any(
        item.region not in pins
        or (item.source.archive.store_id, item.source.archive.batch_id)
        != (inputs.store_id, pins[item.region])
        for item in assets.images
    ):
        raise ValueError("Offline images differ from the pinned regional image batches")
    for pin in inputs.sources:
        frozen = FrozenSources(inputs.archive, inputs.store_id, pin.image_batch)
        if {(scope.provider, scope.kind) for scope in frozen.inventory.scope} != {
            (pin.region, "image")
        }:
            raise ValueError("Offline image pin must be exclusively regional images")
        actual = {item.source.id for item in assets.images if item.region == pin.region}
        if actual != {entry.source_version_id for entry in frozen.inventory.current}:
            raise ValueError("Offline images must cover every current regional source")
    verify_assets(assets, root)
    verify_asset_sources(assets, {inputs.store_id: inputs.archive}, crops=crops)
    return MountedImages(assets, root, crops)
