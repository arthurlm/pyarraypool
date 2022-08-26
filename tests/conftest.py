import tempfile
from uuid import uuid1

import pytest

import pyarraypool


@pytest.fixture(autouse=True)
def autoconfigure_pool() -> None:
    pyarraypool.configure_global_pool(
        link_path=f"{tempfile.gettempdir()}/test_pool_{uuid1()}.seg",
        slot_count=50,
        data_size="64M",
        autostart=False,
    )
