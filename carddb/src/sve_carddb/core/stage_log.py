"""Export stage measurements that survive an interrupted run."""

import os
import pathlib
import resource
import sys
import time

_START = time.monotonic()
_last_wall = _START
_last_cpu = 0.0


def _cpu() -> float:
    own = resource.getrusage(resource.RUSAGE_SELF)
    children = resource.getrusage(resource.RUSAGE_CHILDREN)
    return own.ru_utime + own.ru_stime + children.ru_utime + children.ru_stime


def _rss_gib() -> float:
    with pathlib.Path("/proc/self/status").open(encoding="ascii") as status:
        for line in status:
            if line.startswith("VmRSS:"):
                return int(line.split()[1]) / 1048576
    return 0.0


def mark(stage: str) -> None:
    """Report the segment that ended here, so each line covers the work since the previous mark."""
    global _last_wall, _last_cpu
    wall, cpu = time.monotonic(), _cpu()
    peak = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss / 1048576
    child_peak = resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss / 1048576
    sys.stderr.write(
        f"[stage] {time.strftime('%H:%M:%S')} {stage}: "
        f"{wall - _last_wall:7.1f}s wall {cpu - _last_cpu:7.1f}s cpu "
        f"rss {_rss_gib():5.1f} GiB peak {peak:5.1f} GiB child-peak {child_peak:5.1f} GiB "
        f"total {wall - _START:6.0f}s\n"
    )
    sys.stderr.flush()
    if destination := os.environ.get("SVE_STAGE_LOG"):
        table = pathlib.Path(destination)
        fresh = not table.exists()
        # Close each append so completed stages survive an interrupted export.
        with table.open("a", encoding="utf-8") as out:
            if fresh:
                out.write(
                    "utc\tstage\twall_s\tcpu_s\trss_gib\tpeak_gib\tchild_peak_gib\ttotal_s\n"
                )
            out.write(
                f"{time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())}\t{stage}\t"
                f"{wall - _last_wall:.1f}\t{cpu - _last_cpu:.1f}\t{_rss_gib():.2f}\t"
                f"{peak:.2f}\t{child_peak:.2f}\t{wall - _START:.0f}\n"
            )
    _last_wall, _last_cpu = wall, cpu
