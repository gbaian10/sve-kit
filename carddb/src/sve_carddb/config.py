"""Runtime settings, read from `SVE_*` environment variables."""

from pathlib import Path  # ruff: ignore[typing-only-standard-library-import] -- pydantic reads annotations at runtime

from pydantic import Field
from pydantic_settings import BaseSettings, SettingsConfigDict

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

    @property
    def manifest_path(self) -> Path:
        """The manifest database."""
        return self.data_dir / "manifest" / "manifest.sqlite"

    @property
    def lock_path(self) -> Path:
        """The single-crawler lock file."""
        return self.data_dir / "manifest" / ".lock"
