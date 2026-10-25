# Runtime mounts keep code and data changes from invalidating dependency layers.
# uv 0.13 is the first release shipping Python 3.15, so this is the last one before it.
FROM ghcr.io/astral-sh/uv:0.12.24-python3.14-trixie-slim

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
    --mount=type=bind,source=carddb/pyproject.toml,target=pyproject.toml \
    --mount=type=bind,source=carddb/uv.lock,target=uv.lock \
    uv sync --locked --no-install-project --no-default-groups
# Profiling is available without adding a runtime import dependency.
RUN --mount=type=cache,target=/root/.cache/uv \
    uv pip install --python /opt/venv/bin/python py-spy

ENTRYPOINT ["python", "-c", "from sve_carddb.cli import main; main()"]
