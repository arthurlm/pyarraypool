import logging
import tempfile
import weakref
from contextlib import contextmanager
from pathlib import Path
from typing import Any, Iterator, Optional, Tuple, Union

import numpy as np

from .pyarraypool import ShmObjectPool

MemorySizeType = Union[str, int]

LOGGER = logging.getLogger(__name__)

_GLOBAL_POOL: Optional[ShmObjectPool] = None


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
    python_id = id(arr)

    pool = get_reusable_pool()
    memview = pool.add_object(python_id, arr.size * arr.itemsize)

    out = ndarrayproxy(arr.shape, dtype=arr.dtype, buffer=memview)
    out.python_id = python_id
    out[:] = arr[:]

    weakref.finalize(out, pool.detach_object, python_id)
    return out


def get_reusable_pool() -> ShmObjectPool:
    global _GLOBAL_POOL

    if _GLOBAL_POOL is None:
        raise PoolNotRunning()

    return _GLOBAL_POOL


def start_pool(
    *,
    link_path: Optional[Union[Path, str]] = None,
    **kwargs
) -> None:
    global _GLOBAL_POOL

    path = link_path or f"{tempfile.gettempdir()}/pyarraypool.seg"

    if _GLOBAL_POOL is not None:
        raise PoolAlreadyExists()

    _GLOBAL_POOL = ShmObjectPool(
        path=str(path),
        **kwargs
    )


@contextmanager
def object_pool(
    *,
    slot_count: int = 10_000,
    data_size: MemorySizeType = "512M",
    link_path: Optional[Union[Path, str]] = None,
) -> Iterator[None]:
    global _GLOBAL_POOL
    start_pool(
        link_path=link_path,
        slot_count=slot_count,
        data_size=_parse_datasize_to_bytes(data_size),
    )

    LOGGER.info("Pool attached (data_size: %s)", data_size)
    try:
        yield
    finally:
        _GLOBAL_POOL = None
        LOGGER.info("Pool detached")
