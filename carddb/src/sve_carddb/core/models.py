"""Strict immutable models and shared scalar constraints."""

from typing import Annotated

from pydantic import BaseModel, ConfigDict, Field

from sve_carddb.core.dates import DATE, INSTANT

Text = Annotated[str, Field(min_length=1)]
Hash = Annotated[str, Field(pattern=r"^sha256:[0-9a-f]{64}\Z")]
Date = Annotated[str, Field(pattern="^" + DATE + "$")]
Instant = Annotated[str, Field(pattern="^" + INSTANT + "$")]
UInt = Annotated[int, Field(ge=0, le=9007199254740991)]


class RecordData(BaseModel):
    model_config = ConfigDict(
        extra="forbid", strict=True, frozen=True, regex_engine="python-re"
    )
