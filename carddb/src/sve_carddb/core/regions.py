"""Physical regions and acquisition source namespaces."""

from enum import StrEnum
from typing import Literal

Region = Literal["jp", "en"]


class SourceRegion(StrEnum):
    JP = "jp"
    EN = "en"
    # The digital games' official card lists, kept for mapping SVE cards to digital ones.
    SV1 = "sv1"
    SVWB = "svwb"
