"""Bounded object uploads that admit no new PUT after the first failure."""

from concurrent.futures import FIRST_COMPLETED, Future, ThreadPoolExecutor, wait
from dataclasses import dataclass, field
from itertools import islice
from threading import Event
from time import monotonic
from typing import TYPE_CHECKING

from sve_carddb.export.read_api import ExportError

if TYPE_CHECKING:
    from collections.abc import Callable, Iterable

    from sve_publish.adapter import R2Store

PROGRESS_SECONDS = 5
PROGRESS_OBJECTS = 100


@dataclass(frozen=True)
class UploadObject:
    """Load bytes only when a worker is ready, keeping image memory bounded."""

    key: str
    read: Callable[[], bytes]
    headers: dict[str, str]
    image: bool = False


@dataclass
class UploadPool:
    """Finish admitted operations after failure, without admitting further PUTs."""

    store: R2Store
    jobs: int
    total: int
    progress: Callable[[str], None] | None = None
    written: int = 0
    completed: int = 0
    stopped: Event = field(default_factory=Event)

    def sync(self, objects: Iterable[UploadObject]) -> None:
        """Returning is the phase barrier: every admitted object has finished."""
        self._progress()
        with ThreadPoolExecutor(max_workers=self.jobs) as executor:
            try:
                self._run(executor, objects)
            except BaseException:
                self.stopped.set()
                raise
        self._progress()

    def _run(
        self, executor: ThreadPoolExecutor, objects: Iterable[UploadObject]
    ) -> None:
        remaining = iter(objects)
        last = monotonic()
        last_count = self.completed
        failure: BaseException | None = None
        pending = {
            executor.submit(self._sync, item) for item in islice(remaining, self.jobs)
        }
        while pending:
            done, pending = wait(pending, timeout=1, return_when=FIRST_COMPLETED)
            error = self._collect(done)
            if error is not None and failure is None:
                failure = error
                if self.progress is not None:
                    self.progress("Upload failed; waiting for in-flight objects")
            now = monotonic()
            if (
                now - last >= PROGRESS_SECONDS
                or self.completed - last_count >= PROGRESS_OBJECTS
            ):
                self._progress()
                last = now
                last_count = self.completed
            if not self.stopped.is_set():
                pending.update(
                    executor.submit(self._sync, item)
                    for item in islice(remaining, len(done))
                )
        if failure is not None:
            raise failure

    def _collect(self, done: set[Future[int | None]]) -> BaseException | None:
        failure = None
        for future in done:
            error = future.exception()
            if error is not None:
                failure = error
                continue
            written = future.result()
            if written is not None:
                self.written += written
                self.completed += 1
        return failure

    def _progress(self) -> None:
        if self.progress is not None:
            self.progress(
                f"Upload: {self.completed}/{self.total} complete, {self.written} written"
            )

    def _sync(self, item: UploadObject) -> int | None:
        try:
            if self.stopped.is_set():
                return None
            return self._write(item, item.read())
        except BaseException as error:
            self.stopped.set()
            if isinstance(error, ExportError):
                raise ExportError(f"{error}: {item.key}") from None
            if isinstance(error, OSError):
                raise ExportError(
                    f"Local object preparation failed: {item.key}"
                ) from None
            raise

    def _write(self, item: UploadObject, raw: bytes) -> int | None:
        if item.image:
            if self.stopped.is_set():
                return None
            self.store.put_image(item.key, raw)
            return 1
        # An admitted PUT can still finish after another worker fails.
        if self.stopped.is_set():
            return None
        if self.store.put(
            item.key,
            raw,
            item.headers,
            expected=None,
        ):
            return 1
        stored = self.store.get(item.key)
        if stored is None or stored.raw != raw or stored.headers != item.headers:
            raise ExportError("Immutable JSON object differs from the export")
        return 0
