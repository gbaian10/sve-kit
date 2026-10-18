"""Join pinned image metadata with registered faces for illustration candidates."""

import json
from pathlib import Path  # ruff: ignore[typing-only-standard-library-import] -- Typer resolves command annotations at runtime
from typing import Annotated

import typer
from pydantic import ValidationError

from sve_carddb.core.json import digest, integer, string
from sve_carddb.domains.registry.records import AllocationData, FaceData, PrintingData
from sve_carddb.domains.registry.snapshot import load_registry
from sve_carddb.export.image_index import ImageIndex
from sve_carddb.export.media_urls import image_path
from sve_carddb.images.art_matching import (
    ArtCandidates,
    ArtInput,
    ArtMember,
    compare_art,
)


def art_inputs(
    index: ImageIndex, authored: Path, image_root: Path
) -> tuple[tuple[ArtInput, ...], tuple[str, ...]]:
    """Use permanent image bindings, never guessed card-number filenames."""
    index.validate_tables()
    registry = load_registry(authored)
    printing = {
        item.data.id: item.data
        for item in registry.records.values()
        if isinstance(item.data, PrintingData)
    }
    faces = {
        item.data.id: item.data
        for item in registry.records.values()
        if isinstance(item.data, FaceData)
    }
    allocations = {
        item.data.printing_id: item.data.int_id
        for item in registry.records.values()
        if isinstance(item.data, AllocationData)
    }
    media = {
        (string(row["printing_id"]), string(row["face_id"])): row
        for row in index.tables["printing_image"]
    }
    sizes = {
        string(row["image_id"]): integer(row["bytes"])
        for row in index.tables["image_variant"]
        if row["size_key"] == "art_m"
    }
    result = []
    unavailable = []
    for binding in index.bindings:
        parent, face = printing.get(binding.printing_id), faces.get(binding.face_id)
        if parent is None:
            raise ValueError(f"Unknown printing in image index: {binding.printing_id}")
        if face is None:
            raise ValueError(f"Unknown face in image index: {binding.face_id}")
        if (
            parent.card_id != face.card_id
            or allocations[parent.id] != binding.int_id
            or face.ordinal != binding.ordinal
            or face.id not in {mapping.face_id for mapping in parent.source_face_map}
        ):
            raise ValueError("Art image binding differs from registered face")
        row = media[parent.id, face.id]
        if row["availability"] != "available" or row["publication_state"] != "approved":
            unavailable.append(parent.id + "/" + face.id)
            continue
        path = image_root / image_path(binding.int_id, binding.ordinal, "art_m")
        if path.is_symlink() or not path.resolve().is_relative_to(image_root.resolve()):
            raise ValueError("Art bitmap escapes image root")
        raw = path.read_bytes()
        if len(raw) != sizes[string(row["image_id"])]:
            raise ValueError("Art bitmap size differs from pinned image index")
        result.append(
            ArtInput(
                ArtMember(
                    printing_id=parent.id,
                    face_id=face.id,
                    card_id=parent.card_id,
                    region=parent.region,
                    card_no=parent.card_no,
                    int_id=binding.int_id,
                    ordinal=binding.ordinal,
                    image_sha256=digest(raw),
                ),
                path,
            )
        )
    return tuple(result), tuple(sorted(unavailable))


def art_candidates(
    image_index: Annotated[Path, typer.Option(help="Pinned producer image index.")],
    image_root: Annotated[
        Path, typer.Option(help="Read-only root containing images/art_m.")
    ],
    authored: Annotated[Path, typer.Option(help="Complete registered identities.")],
    output: Annotated[
        Path, typer.Option(help="Private candidate JSON file; never authored adoption.")
    ],
    previous: Annotated[
        Path | None, typer.Option(help="Reuse scores for unchanged old members.")
    ] = None,
) -> None:
    """Compare art_m images of the same face; emit groups, scores and review pairs."""
    paths = [
        image_index,
        image_root,
        authored,
        output,
        *([previous] if previous else []),
    ]
    if any(not path.is_absolute() for path in paths):
        raise typer.BadParameter("All art candidate paths must be absolute")
    if any(
        output.resolve().is_relative_to(root.resolve())
        for root in (image_root, authored)
    ) or output.resolve() in {
        path.resolve() for path in (image_index, previous) if path
    }:
        raise typer.BadParameter("Candidate output must be separate from input data")
    try:
        index = ImageIndex.read(image_index)
        inputs, unavailable = art_inputs(index, authored, image_root)
        result = compare_art(
            inputs,
            image_index_sha256=digest(index.content()),
            unavailable=unavailable,
            previous=ArtCandidates.read(previous) if previous else None,
        )
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_bytes(result.candidates.content())
    except ModuleNotFoundError as error:
        if error.name not in {"cv2", "numpy"}:
            raise
        raise typer.BadParameter(
            "Install sve-carddb[art-matching] to compare images"
        ) from None
    except ValidationError as error:
        details = "; ".join(
            ".".join(map(str, item["loc"])) + ": " + item["msg"]
            for item in error.errors(
                include_input=False, include_context=False, include_url=False
            )
        )
        raise typer.BadParameter(f"Invalid art comparison input: {details}") from error
    except KeyError as error:
        raise typer.BadParameter(
            f"Missing registry or image-index field/reference (printing, face or image): {error}"
        ) from error
    except (ValueError, OSError) as error:
        raise typer.BadParameter(f"Invalid art comparison input: {error}") from error
    candidate = result.candidates
    typer.echo(
        json.dumps(
            {
                "members": len(candidate.members),
                "unavailable": len(candidate.unavailable),
                "groups": len(candidate.groups),
                "compared_pairs": result.compared_pairs,
                "reused_pairs": result.reused_pairs,
                "same_language_review": sum(
                    pair.scope == "same_language" for pair in candidate.review
                ),
                "cross_language_review": sum(
                    pair.scope == "cross_language" for pair in candidate.review
                ),
                "conflicts": len(candidate.conflicts),
            }
        )
    )
