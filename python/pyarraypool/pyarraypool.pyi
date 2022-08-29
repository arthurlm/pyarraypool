from typing import Optional


class ShmObjectPool:
    def __init__(
        self, *,
        slot_count: int = 5000,
        data_size: int = 524288000,
        path: str = "pyarraypool.seg",
    ) -> None:
        ...

    def add_object(self, python_id: int, request_size: int) -> memoryview:
        ...

    def attach_object(self, python_id: int) -> memoryview:
        ...

    def detach_object(self, python_id: int) -> None:
        ...

    def set_object_releasable(self, python_id: int) -> None:
        ...

    def memview_of(self, python_id: int) -> Optional[memoryview]:
        ...

    def dump(self) -> str:
        ...
