"""Current checker closure is independent of a policy's historical background."""

from pathlib import Path
from typing import TYPE_CHECKING

from sve_carddb.snapshot.values import digest
from sve_carddb.translations.sources import RUNTIME as TRANSLATION_RUNTIME

if TYPE_CHECKING:
    from sve_carddb.translations.sources import Sources

RUNTIME = tuple(
    sorted(
        {
            *TRANSLATION_RUNTIME,
            "carddb/src/sve_carddb/cli.py",
            "carddb/src/sve_carddb/extract/compare_jp.py",
            "carddb/src/sve_carddb/registry/review.py",
            *(
                "carddb/src/sve_carddb/digital_name_policies/" + name + ".py"
                for name in (
                    "__init__",
                    "models",
                    "loader",
                    "evaluate",
                    "report",
                    "commands",
                    "runtime",
                    "owners",
                    "application",
                )
            ),
        }
    )
)


def require_runtime(sources: Sources) -> None:
    """Verify loaded checker bytes; do not rewrite or invalidate historical recipes."""
    declared = {pin.name: pin.sha256 for pin in sources.build.dependencies}
    root = Path(__file__).resolve().parents[4]
    if sources.historical or any(
        (root / name).is_symlink()
        or declared.get(name) != digest((root / name).read_bytes())
        for name in RUNTIME
    ):
        raise ValueError("Digital-name policy runtime dependency closure mismatch")
