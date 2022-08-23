import multiprocessing
import pickle
import tempfile
from uuid import uuid1
import pyarraypool
import pytest

import numpy as np


class TestParseDataSize:
    @pytest.mark.parametrize("value,expected", [
        ("1G", 1024 ** 3),
        ("2.4G", int(2.4 * 1024 ** 3)),
        ("5M", 5 * 1024 ** 2),
        ("1K", 1024),
        ("1.5K", 1.5 * 1024),
        (1.5, 1),
        (256, 256),
    ])
    def test_valid(self, value: pyarraypool.MemorySizeType, expected: int) -> None:
        assert pyarraypool._parse_datasize_to_bytes(value) == expected

    @pytest.mark.parametrize("value", [
        -5,
        0,
        "0",
        "0.8",
        "0G",
    ])
    def test_null_value(self, value: pyarraypool.MemorySizeType) -> None:
        with pytest.raises(ValueError, match="pool size cannot be null or less"):
            pyarraypool._parse_datasize_to_bytes(value)

    @pytest.mark.parametrize("value", [
        "Awesome",
        "FewG",
    ])
    def test_invalid_value(self, value: pyarraypool.MemorySizeType) -> None:
        with pytest.raises(ValueError):
            pyarraypool._parse_datasize_to_bytes(value)


@pytest.fixture
def link_path() -> str:
    return f"{tempfile.gettempdir()}/test_pool_{uuid1()}.seg"


def assert_pool_off() -> None:
    with pytest.raises(pyarraypool.PoolNotRunning):
        pyarraypool.get_reusable_pool()


def assert_pool_on() -> None:
    assert pyarraypool.get_reusable_pool() is not None


class TestPoolLifecycle:
    def test_basic(self, link_path: str) -> None:
        # Pool off
        assert_pool_off()

        with pyarraypool.object_pool(link_path=link_path):
            # Pool running
            assert_pool_on()

        # Pool stopped
        assert_pool_off()

    def test_init_twice(self, link_path: str) -> None:
        assert_pool_off()

        with pyarraypool.object_pool(link_path=link_path):
            assert_pool_on()

            # Check error is raised
            with pytest.raises(pyarraypool.PoolAlreadyExists):
                with pyarraypool.object_pool(link_path=link_path):
                    ...

            # Check if pool is still running
            assert_pool_on()

        assert_pool_off()

    def test_init_and_release_multiple(self, link_path: str) -> None:
        assert_pool_off()

        with pyarraypool.object_pool(link_path=link_path):
            assert_pool_on()
        assert_pool_off()

        with pyarraypool.object_pool(link_path=link_path):
            assert_pool_on()
        assert_pool_off()

        with pyarraypool.object_pool(link_path=link_path):
            assert_pool_on()
        assert_pool_off()

    def test_attach_not_running(self, link_path: str) -> None:
        with pytest.raises(pyarraypool.PoolNotRunning):
            pyarraypool.get_reusable_pool()

    def test_clear_if_crash_and_attach(self, link_path: str) -> None:
        assert_pool_off()

        with pytest.raises(ValueError):
            with pyarraypool.object_pool(link_path=link_path):
                assert_pool_on()
                raise ValueError(":(")

        assert_pool_off()


class TestArrayProxy:
    @pytest.fixture(autouse=True)
    def shm_ctx(self, link_path):
        with pyarraypool.object_pool(link_path=link_path):
            yield

    def test_has_python_id(self):
        arr = np.arange(5)
        proxy = pyarraypool.make_transferable(arr)
        assert proxy.python_id != 0

    def test_getattr(self):
        arr = np.arange(5)
        proxy = pyarraypool.make_transferable(arr)
        assert arr.min() == proxy.min() == 0
        assert arr.max() == proxy.max() == 4

    def test_get_item(self):
        arr = np.arange(5)
        proxy = pyarraypool.make_transferable(arr)
        assert arr[0] == proxy[0]
        assert (arr == proxy).all()

    def test_other_attributes(self):
        arr = np.arange(5)
        proxy = pyarraypool.make_transferable(arr)
        assert len(arr) == len(proxy)

    def test_multiple_dimensions(self):
        arr = np.arange(10).reshape(2, 5)
        proxy = pyarraypool.make_transferable(arr)
        assert proxy.shape == (2, 5)
        assert (arr == proxy).all()

    def test_pickling(self):
        arr = np.arange(1000)
        proxy = pyarraypool.make_transferable(arr)

        raw_arr = pickle.dumps(arr)
        raw_proxy = pickle.dumps(proxy)
        assert len(raw_proxy) < len(raw_arr)

        arr2 = pickle.loads(raw_arr)
        proxy2 = pickle.loads(raw_proxy)

        assert (arr == arr2).all()
        assert (proxy == proxy2).all()
        assert (arr == proxy2).all()

    def test_subprocess(self):
        arr = np.arange(50)
        proxy = pyarraypool.make_transferable(arr)

        with multiprocessing.Pool(5) as pool:
            pool.starmap(add_one, [
                (proxy, i) for i in range(proxy.size)
            ])

        assert (proxy == arr + 1).all()


def add_one(arr, idx):
    arr[idx] += 1
