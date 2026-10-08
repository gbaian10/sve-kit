"""Historical text reads do not borrow current or escape a frozen batch."""

import re
from typing import TYPE_CHECKING

import pytest

from sve_carddb.domains.text_observations import FrozenTexts
from sve_carddb.ingest.archive.source_archive import ArchiveError

from .test_effect_presence import card_from_raw, page

if TYPE_CHECKING:
    from pathlib import Path


@pytest.mark.parametrize("change", ["none", "region", "number", "version"])
def test_explicit_historical_version_has_independent_frozen_identity(
    tmp_path: Path, change: str
) -> None:
    original = card_from_raw(
        tmp_path, page("jp", '<div class="detail">Synthetic historical rule</div>')
    )
    pin = original.source.archive
    provider = FrozenTexts(
        tmp_path / "archive",
        pin.store_id,
        pin.batch_id,
        region="jp",
        parser_version=original.source.parser_version,
    )
    provider.current.clear()
    assert provider.card("jp", "SYN-01") is None
    if change == "none":
        assert provider.version("jp", "SYN-01", original.source.id) == original
    else:
        message = {
            "region": "Frozen historical text region mismatch",
            "number": "Frozen text source identity/media mismatch",
            "version": "Source version is absent from pinned batch",
        }[change]
        with pytest.raises(
            (ValueError, ArchiveError),
            match=rf"\A{re.escape(message)}\Z",
        ):
            provider.version(
                "en" if change == "region" else "jp",
                "SYN-02" if change == "number" else "SYN-01",
                "src:v1:" + "f" * 64 if change == "version" else original.source.id,
            )
