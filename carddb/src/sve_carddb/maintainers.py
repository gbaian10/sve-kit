"""Repository-controlled reviewer identities shared by approval boundaries."""

import tomllib
from functools import cache
from importlib.resources import files
from typing import Annotated

from pydantic import AfterValidator, BaseModel, ConfigDict, Field, field_validator

Username = Annotated[
    str, Field(pattern=r"^[A-Za-z0-9](?:[A-Za-z0-9-]{0,37}[A-Za-z0-9])?\Z")
]


class _Configuration(BaseModel):
    model_config = ConfigDict(
        extra="forbid", strict=True, frozen=True, regex_engine="python-re"
    )

    maintainers: Annotated[list[Username], Field(min_length=1)]

    @field_validator("maintainers")
    @classmethod
    def unique(cls, value: list[str]) -> list[str]:
        if len({name.casefold() for name in value}) != len(value):
            raise ValueError("Maintainer identities must be unique")
        return value


@cache
def listed() -> frozenset[str]:
    """Load only the shipped policy; environment variables cannot extend authority."""
    content = (
        files("sve_carddb").joinpath("maintainers.toml").read_text(encoding="utf-8")
    )
    config = _Configuration.model_validate(tomllib.loads(content))
    return frozenset(config.maintainers)


def is_maintainer(value: object) -> bool:
    """Require an exact listed identity, including at the SQLite value boundary."""
    return isinstance(value, str) and value in listed()


def require_maintainer(value: str) -> str:
    """Keep receipt eligibility tied to the same policy as loader and SQL checks."""
    if not is_maintainer(value):
        raise ValueError("Reviewer must be a repository-listed maintainer")
    return value


Maintainer = Annotated[str, AfterValidator(require_maintainer)]
