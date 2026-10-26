"""Share real preparation within one pytest run, restoring detached Python graphs."""

import fcntl
import pickle  # ruff: ignore[suspicious-pickle-import] -- cache only trusted objects generated in this run's private pytest tree
from functools import cache
from io import BytesIO
from types import MappingProxyType
from typing import TYPE_CHECKING

from sve_carddb.domains.parsed_cards.reader import read
from sve_carddb.domains.parsed_cards.storage import Catalog

if TYPE_CHECKING:
    from collections.abc import Callable, Mapping
    from pathlib import Path


def mapping_proxy(values: dict[object, object]) -> MappingProxyType[object, object]:
    return MappingProxyType(values)


def reduce_mapping(
    value: Mapping[object, object],
) -> tuple[Callable[..., object], tuple[object, ...]]:
    return mapping_proxy, (dict(value),)


@cache
def public_catalog(root: Path) -> Catalog:
    return read(root)


def freeze(value: object, *, catalog_root: Path | None = None) -> bytes:
    stream = BytesIO()
    writer = pickle.Pickler(stream, protocol=pickle.HIGHEST_PROTOCOL)
    # Reducers are local: production serialization and other tests keep their defaults.
    writer.dispatch_table = {
        MappingProxyType: reduce_mapping,
    }
    if catalog_root is not None:
        # Pickling the full admitted catalog duplicates its interned data and pressures RAM.
        # Read it once per worker; fixtures only mutate their own sources and databases.
        writer.dispatch_table |= {
            Catalog: lambda _: (public_catalog, (catalog_root,)),
        }
    writer.dump(value)
    return stream.getvalue()


def restore[T](content: bytes, kind: type[T]) -> T:
    # Only bytes produced by freeze in this run's private pytest temp tree are read.
    value: object = pickle.loads(content)  # ruff: ignore[suspicious-pickle-usage] -- no persistent or caller-supplied pickle inputs
    assert isinstance(value, kind)
    return value


def shared_preparation(path: Path, build: Callable[[], bytes]) -> bytes:
    """Publish only complete preparation; a failed producer leaves no cached value."""
    with path.with_suffix(".lock").open("a+b") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        if not path.exists():
            temporary = path.with_suffix(".tmp")
            temporary.write_bytes(build())
            temporary.replace(path)
        return path.read_bytes()
