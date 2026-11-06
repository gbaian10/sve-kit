# Runtime mounts keep code and data changes from invalidating dependency layers.
FROM python:3.14.10-slim-trixie
COPY --from=ghcr.io/astral-sh/uv:latest /uv /uvx /bin/

RUN apt-get update && apt-get install --no-install-recommends -y git \
    && rm -rf /var/lib/apt/lists/*

# The environment must be readable by the caller's uid from Compose.
ENV UV_PYTHON_DOWNLOADS=0 \
    UV_COMPILE_BYTECODE=1 \
    UV_LINK_MODE=copy \
    UV_PROJECT_ENVIRONMENT=/opt/venv \
    PATH=/opt/venv/bin:${PATH} \
    PYTHONDONTWRITEBYTECODE=1

WORKDIR /opt/build
# Cached wheels avoid downloads when rebuilding dependencies.
RUN --mount=type=cache,target=/root/.cache/uv \
    --mount=type=bind,source=pyproject.toml,target=pyproject.toml \
    --mount=type=bind,source=uv.lock,target=uv.lock \
    --mount=type=bind,source=carddb/pyproject.toml,target=carddb/pyproject.toml \
    --mount=type=bind,source=publish/pyproject.toml,target=publish/pyproject.toml \
    uv sync --package sve-carddb --locked --no-install-workspace --no-default-groups
# Profiling is available without adding a runtime import dependency.
RUN --mount=type=cache,target=/root/.cache/uv \
    uv pip install --python /opt/venv/bin/python py-spy

ENTRYPOINT ["python", "-c", "from sve_carddb.cli import main; main()"]
