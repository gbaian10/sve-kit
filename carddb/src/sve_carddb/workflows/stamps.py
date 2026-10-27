"""Read local source images for stamp candidates without opening a crawl manifest."""

from collections import defaultdict
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Annotated, Literal

import typer
from PIL import Image
from pydantic import JsonValue, TypeAdapter

from sve_carddb.core.json import canonical
from sve_carddb.core.models import RecordData, Text
from sve_carddb.core.regions import SourceRegion
from sve_carddb.domains.parsed_cards.reader import read
from sve_carddb.domains.registry.marks import load_marks
from sve_carddb.domains.registry.records import ArtData, PrintingData
from sve_carddb.domains.registry.transitions.renewal import load_candidate_registry
from sve_carddb.parse.pages.official_jp import image_path


@dataclass(frozen=True)
class StampInput:
    printing: PrintingData
    face_id: str
    image: Path
    references: tuple[Path, ...]


class MarkExample(RecordData):
    printing_id: Text
    label: Text
    box: tuple[int, int, int, int]

    @property
    def bounds(self) -> tuple[int, int, int, int]:
        """Require a nonempty normalized rectangle inside the source image."""
        x, y, width, height = self.box
        if min(x, y) < 0 or min(width, height) <= 0:
            raise ValueError("Invalid mark example rectangle")
        return self.box


def stamp_inputs(  # ruff: ignore[complex-structure] -- one join owns registered faces, exact source URLs and same-art references
    authored: Path, parsed_cards: Path, data_dir: Path
) -> tuple[StampInput, ...]:
    """Join registered same-art uses to the exact parsed img src, including suffixes."""
    catalog = read(parsed_cards)
    current = {
        key
        for batch in catalog.batches
        for key, selected in batch.versions.items()
        if selected
    }
    images = {}
    for pair in catalog.files:
        for card in pair.cards:
            for observation in card.observations:
                if observation.evidence.descriptor.id not in current:
                    continue
                document = observation.document
                images[document.region, card.card_no] = tuple(
                    data_dir / image_path(face.image, SourceRegion(document.region))
                    for face in document.record.faces
                )
    registry = load_candidate_registry(authored)
    printings = {
        record.data.id: record.data
        for record in registry.records.values()
        if isinstance(record.data, PrintingData)
    }
    paths = {
        (printing.id, mapping.face_id): images[printing.region, printing.card_no][
            mapping.source_index
        ]
        for printing in printings.values()
        if (printing.region, printing.card_no) in images
        for mapping in printing.source_face_map
    }
    groups: dict[tuple[str, str], list[tuple[str, str]]] = defaultdict(list)
    for record in registry.records.values():
        if isinstance(record.data, ArtData):
            uses = [(use.printing_id, use.face_id) for use in record.data.uses]
            for use in uses:
                groups[use].extend(other for other in uses if other != use)
    result = []
    for printing in sorted(
        printings.values(),
        key=lambda value: (value.region, value.card_no, value.variant_key),
    ):
        if printing.home_set_id != "PR" and not any(
            paths.get((printing.id, mapping.face_id), Path()).parent.name == "PR"
            for mapping in printing.source_face_map
        ):
            continue
        for mapping in printing.source_face_map:
            key = printing.id, mapping.face_id
            path = paths.get(key)
            if path is None or not path.is_file():
                raise ValueError(
                    f"Missing source image for {printing.region}/{printing.card_no}"
                )
            references = sorted(
                (
                    use
                    for use in groups[key]
                    if use in paths and paths[use] != path and paths[use].is_file()
                ),
                key=lambda use: (
                    printings[use[0]].home_set_id == "PR",
                    printings[use[0]].region != printing.region,
                    use,
                ),
            )
            result.append(
                StampInput(
                    printing,
                    mapping.face_id,
                    path,
                    tuple(paths[use] for use in references[:5]),
                )
            )
    return tuple(result)


def detect_stamps(  # ruff: ignore[complex-structure, too-many-arguments, too-many-positional-arguments] -- the command owns explicit source/output paths, optional examples and reviewed-scope selection
    authored: Annotated[Path, typer.Option(help="Registered authored root.")],
    parsed_cards: Annotated[
        Path, typer.Option(help="Admitted parsed-card input root.")
    ],
    output: Annotated[
        Path, typer.Option(help="Private review directory outside source data.")
    ],
    data_dir: Annotated[
        Path, typer.Option(envvar="SVE_DATA_DIR", help="Read-only source cache root.")
    ],
    scope: Annotated[Literal["pr"], typer.Option()] = "pr",
    examples: Annotated[
        Path | None,
        typer.Option(
            help="Private reviewed pattern examples: printing_id, label and [x,y,width,height]."
        ),
    ] = None,
    unreviewed_only: Annotated[
        bool,
        typer.Option(
            help="Scan only printings absent from the reviewed authored marks."
        ),
    ] = False,
) -> None:
    """Emit local difference/gold crops for human confirmation, never authored adoption."""
    if any(
        not path.is_absolute()
        for path in (
            authored,
            parsed_cards,
            data_dir,
            output,
            *([examples] if examples is not None else []),
        )
    ):
        raise typer.BadParameter("All stamp paths must be absolute")
    if any(
        output.resolve().is_relative_to(path.resolve())
        or path.resolve().is_relative_to(output.resolve())
        for path in (authored, parsed_cards, data_dir)
    ):
        raise typer.BadParameter("Candidate output must be separate from source data")
    try:
        from sve_carddb.images.stamps import (  # ruff: ignore[import-outside-top-level] -- OpenCV remains optional for normal builds and CLI help
            MarkBox,
            MarkGroups,
            bitmap,
            detect_marks,
            mark_pattern,
            match_patterns,
        )
    except ModuleNotFoundError as error:
        if error.name not in {"cv2", "numpy"}:
            raise
        raise typer.BadParameter(
            "Install sve-carddb[art-matching] for stamp detection"
        ) from None
    inputs = stamp_inputs(authored, parsed_cards, data_dir)
    patterns = []
    if examples is not None:
        if output.resolve() == examples.resolve() or examples.resolve().is_relative_to(
            output.resolve()
        ):
            raise typer.BadParameter("Examples must be separate from candidate output")
        by_id = {item.printing.id: item.image for item in inputs}
        for example in TypeAdapter(tuple[MarkExample, ...]).validate_json(
            examples.read_bytes()
        ):
            if example.printing_id not in by_id:
                raise typer.BadParameter(
                    "Unknown example printing_id: " + example.printing_id
                )
            patterns.append(
                mark_pattern(
                    by_id[example.printing_id].read_bytes(),
                    MarkBox(*example.bounds),
                    label=example.label,
                )
            )
    if unreviewed_only:
        reviewed = {record.printing_id for record in load_marks(authored)}
        inputs = tuple(item for item in inputs if item.printing.id not in reviewed)
    output.mkdir(parents=True, exist_ok=True)
    rows: list[JsonValue] = []
    grouping = MarkGroups()
    for index, item in enumerate(inputs):
        raw = item.image.read_bytes()
        detection = detect_marks(
            raw, tuple(path.read_bytes() for path in item.references)
        )
        matching = match_patterns(raw, tuple(patterns)) if patterns else ()
        name = (
            item.printing.region
            + "-"
            + item.printing.id.removeprefix("p:")
            + "-"
            + item.face_id.removeprefix("f:")
        )
        normalized = bitmap(raw)
        image = Image.fromarray(normalized[:, :, ::-1])
        crops: list[JsonValue] = []
        for number, box in enumerate(detection.boxes):
            filename = f"{name}-{number:02}.png"
            image.crop(
                (
                    max(0, box.x - 8),
                    max(0, box.y - 8),
                    min(image.width, box.x + box.width + 8),
                    min(image.height, box.y + box.height + 8),
                )
            ).save(output / filename)
            crops.append(filename)
        rows.append(
            {
                "printing_id": item.printing.id,
                "face_id": item.face_id,
                "region": item.printing.region,
                "card_no": item.printing.card_no,
                "source_image": str(item.image),
                "reference": str(item.references[detection.reference_index])
                if detection.reference_index is not None
                else None,
                "method": detection.method,
                "inliers": detection.inliers,
                "boxes": [dict[str, JsonValue](asdict(box)) for box in detection.boxes],
                "groups": [grouping.add(normalized, box) for box in detection.boxes],
                "pattern_matches": [
                    dict[str, JsonValue](asdict(match)) for match in matching
                ],
                "crops": crops,
            }
        )
        if (index + 1) % 50 == 0:
            typer.echo(f"Scanned {index + 1}/{len(inputs)}")
    (output / "candidates.json").write_bytes(
        canonical({"scope": scope, "review_required": True, "cards": rows})
    )
    typer.echo(
        f"Scanned {len(inputs)} PR faces; review candidates before editing authored data"
    )
