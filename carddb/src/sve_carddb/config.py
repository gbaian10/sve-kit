"""Runtime settings, read from `SVE_*` environment variables."""

import os
from pathlib import Path  # ruff: ignore[typing-only-standard-library-import] -- pydantic reads annotations at runtime
from typing import Annotated

from pydantic import Field, field_validator
from pydantic_settings import BaseSettings, NoDecode, SettingsConfigDict

# The official sites answer 404 to requests without a browser User-Agent.
BROWSER_USER_AGENT = (
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 "
    "(KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36"
)


class Settings(BaseSettings):
    """Crawler settings. `SVE_DATA_DIR` is required."""

    model_config = SettingsConfigDict(env_prefix="SVE_", frozen=True)

    data_dir: Path
    interval: float = Field(default=2.5, ge=2.0)
    jitter: float = Field(default=0.5, ge=0.0)
    timeout: float = Field(default=30.0, gt=0.0)
    user_agent: str = BROWSER_USER_AGENT
    breaker_threshold: int = Field(default=5, ge=1)
    extra_roots: Annotated[tuple[Path, ...], NoDecode] = ()
    """`SVE_EXTRA_ROOTS`: absolute paths, joined by `os.pathsep`, that symlinks
    under the data root may point into. Only `manifest check` reads through them."""

    @field_validator("extra_roots", mode="before")
    @classmethod
    def _split_roots(cls, value: object) -> object:
        if isinstance(value, str):
            value = [part for part in value.split(os.pathsep) if part]
        return value

    @field_validator("extra_roots")
    @classmethod
    def _require_absolute(cls, value: tuple[Path, ...]) -> tuple[Path, ...]:
        # A relative root would change meaning with the working directory.
        for root in value:
            if not root.is_absolute():
                msg = f"extra root must be an absolute path: {root}"
                raise ValueError(msg)
        return value

    @property
    def manifest_path(self) -> Path:
        """The manifest database."""
        return self.data_dir / "manifest" / "manifest.sqlite"

    @property
    def lock_path(self) -> Path:
        """The single-crawler lock file."""
        return self.data_dir / "manifest" / ".lock"
