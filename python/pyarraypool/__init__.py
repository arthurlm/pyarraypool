import logging
import os
import tempfile
import weakref
from contextlib import contextmanager
from pathlib import Path
from typing import Any, Iterator, Optional, Tuple, Union

import numpy as np

from .pyarraypool import ShmObjectPool

MemorySizeType = Union[str, int]

LOGGER = logging.getLogger(__name__)

_MAX_PYTHON_ID = np.iinfo(np.int64).max

_GLOBAL_POOL: Optional[ShmObjectPool] = None
_CFG_LINK_PATH = f"{tempfile.gettempdir()}/pyarraypool.seg"
_CFG_SLOT_COUNT: int = 10_000
_CFG_DATA_SIZE: int = 512 * (1024 ** 2)
_CFG_AUTOSTART: bool = True


class PoolAlreadyExists(Exception):
    """Pool already exists."""


class PoolNotRunning(Exception):
    """Pool not running."""


def _parse_datasize_to_bytes(value: MemorySizeType) -> int:
    def ensure_positive_value(x: object) -> int:
        assert isinstance(x, (float, int))
        if int(x) <= 0:
            raise ValueError("pool size cannot be null or less", x)
        return int(x)

    if isinstance(value, str):
        if value.endswith("G"):
            return ensure_positive_value(float(value[:-1]) * (1024 ** 3))
        elif value.endswith("M"):
            return ensure_positive_value(float(value[:-1]) * (1024 ** 2))
        elif value.endswith("K"):
            return ensure_positive_value(float(value[:-1]) * 1024)
        else:
            return ensure_positive_value(float(value))

    return ensure_positive_value(value)


class ndarrayproxy(np.ndarray):
    python_id: int = 0

    def __reduce__(self) -> Tuple[Any, ...]:
        # Check: https://docs.python.org/3/library/pickle.html#object.__reduce__
        if self.python_id == 0:
            raise ValueError("Cannot transfer none registered object (did you try sending slice ?)")

        return (
            # Builder
            ndarrayproxy._shm_reconstruct,
            # Builder args
            (self.shape, self.dtype, self.python_id),
        )

    def __getstate__(self):
        raise NotImplementedError()

    def __setstate__(self, state) -> None:
        raise NotImplementedError()

    @classmethod
    def _shm_reconstruct(cls, shape, dtype, python_id) -> "ndarrayproxy":
        pool = get_reusable_pool()
        memview = pool.attach_object(python_id)

        out = cls(shape, dtype=dtype, buffer=memview)

        weakref.finalize(out, pool.detach_object, python_id)
        return out


def make_transferable(arr: np.ndarray) -> ndarrayproxy:
    python_id = hash((id(arr), os.getpid())) % _MAX_PYTHON_ID

    pool = get_reusable_pool()

    if pool.memview_of(python_id) is None:
        # Check if object is already registered in pool
        memview = pool.add_object(python_id, arr.size * arr.itemsize)
        data_not_set = True
    else:
        # Otherwise attach to memory view
        memview = pool.attach_object(python_id)
        assert memview.nbytes == arr.size * arr.itemsize
        data_not_set = False

    # Create proxy object
    out = ndarrayproxy(arr.shape, dtype=arr.dtype, buffer=memview)
    out.python_id = python_id

    # Set data
    if data_not_set:
        out[:] = arr[:]

    weakref.finalize(out, pool.detach_object, python_id)
    return out


def get_reusable_pool() -> ShmObjectPool:
    global _GLOBAL_POOL, _CFG_AUTOSTART

    if _GLOBAL_POOL is None and _CFG_AUTOSTART:
        start_pool()

    if _GLOBAL_POOL is None:
        raise PoolNotRunning()

    return _GLOBAL_POOL


def start_pool() -> None:
    global _GLOBAL_POOL, _CFG_LINK_PATH, _CFG_DATA_SIZE, _CFG_SLOT_COUNT

    if _GLOBAL_POOL is not None:
        raise PoolAlreadyExists()

    _GLOBAL_POOL = ShmObjectPool(
        path=_CFG_LINK_PATH,
        data_size=_CFG_DATA_SIZE,
        slot_count=_CFG_SLOT_COUNT,
    )
    LOGGER.info("Pool attached (data_size: %d bytes, slot_count: %d)", _CFG_DATA_SIZE, _CFG_SLOT_COUNT)


def stop_pool() -> None:
    global _GLOBAL_POOL

    _GLOBAL_POOL = None
    LOGGER.info("Pool detached")


def configure_global_pool(
    *,
    link_path: Optional[Union[Path, str]] = None,
    slot_count: Optional[int] = None,
    data_size: Optional[MemorySizeType] = None,
    autostart: Optional[bool] = None
) -> None:
    global _CFG_LINK_PATH, _CFG_SLOT_COUNT, _CFG_DATA_SIZE, _CFG_AUTOSTART

    if link_path is not None:
        _CFG_LINK_PATH = str(link_path)

    if slot_count is not None:
        _CFG_SLOT_COUNT = slot_count

    if data_size is not None:
        _CFG_DATA_SIZE = _parse_datasize_to_bytes(data_size)

    if autostart is not None:
        _CFG_AUTOSTART = autostart


@contextmanager
def object_pool_context() -> Iterator[None]:
    start_pool()

    try:
        yield
    finally:
        stop_pool()


def cleanup_shm():
    """Remove previous SHM if any."""
    link_path = Path(_CFG_LINK_PATH)
    if not link_path.exists():
        return

    segment_name = link_path.read_text().strip("/")
    segment_path = Path("/dev/shm") / segment_name

    LOGGER.info("Removing SHM segment: %s", segment_path)
    link_path.unlink(missing_ok=True)
    segment_path.unlink(missing_ok=True)
