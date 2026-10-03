"""Durable, locked revision reservations for isolated local previews only."""

import fcntl
import os
from contextlib import contextmanager
from typing import TYPE_CHECKING

from sve_carddb.snapshot.values import canonical, integer, object_value, parse

if TYPE_CHECKING:
    from collections.abc import Iterator

    from pydantic import JsonValue

    from sve_carddb.snapshot.preview import Roots


MAX_SAFE = 9007199254740991


@contextmanager
def reservation(roots: Roots) -> Iterator[tuple[int, JsonValue]]:
    """Hold the local release lock until the caller seals and commits its preview.

    This journal is not a production/R2 allocator. Failed reservations remain in
    the journal; only an explicit committed state supplies unchanged image tokens.
    """
    path = roots.destination("private/media-revisions.jsonl")
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a+b") as stream:
        fcntl.flock(stream.fileno(), fcntl.LOCK_EX)
        stream.seek(0)
        revision = 0
        for line in stream:
            value = object_value(parse(line))
            if integer(value["revision"]) != revision + 1:
                raise ValueError("Preview media reservation journal is not monotonic")
            revision += 1
        if revision >= MAX_SAFE:
            raise ValueError("Preview media revision domain exhausted")
        stream.seek(0, os.SEEK_END)
        stream.write(canonical({"revision": revision + 1}) + b"\n")
        stream.flush()
        os.fsync(stream.fileno())
        state_path = roots.destination("private/media-committed.json")
        state = parse(state_path.read_bytes()) if state_path.exists() else None
        yield revision + 1, state
