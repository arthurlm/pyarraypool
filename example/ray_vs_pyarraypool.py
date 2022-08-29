import contextlib
import numpy as np
import ray
import multiprocessing
import pyarraypool
import time

TEST_COUNT = 100


@ray.remote
def sum_matrix(matrix):
    return int(np.sum(matrix))


def sum_matrix_pyarraypool(matrix):
    return int(np.sum(matrix))


def bench(f):
    @contextlib.wraps(f)
    def wrapper(*args, **kwargs):
        results = np.zeros(TEST_COUNT)

        for test_id in range(TEST_COUNT):
            t_start = time.perf_counter() * 1000.0
            f(*args, **kwargs)
            t_end = time.perf_counter() * 1000.0

            results[test_id] = t_end - t_start

        print(f"Duration for {f.__name__}: {results.mean():.3f}ms")

    return wrapper


@contextlib.contextmanager
def warmup_pool():
    ray.get(sum_matrix.remote(np.ones((100, 100))))

    with multiprocessing.get_context("spawn").Pool() as pool:
        pool.apply(sum_matrix_pyarraypool, args=(np.ones(10),))
        yield pool


@bench
def bench_litteral_argument_value(data):
    _ = ray.get(sum_matrix.remote(data))


@bench
def bench_large_array_object_store(data):
    matrix_ref = ray.put(data)
    _ = ray.get(sum_matrix.remote(matrix_ref))


@bench
def bench_pyarraypool_mp(data, pool):
    matrix_ref = pyarraypool.make_transferable(data)
    _ = pool.apply(sum_matrix_pyarraypool, args=(matrix_ref,))


@bench
def bench_pyarraypool_ray(data):
    matrix_ref = pyarraypool.make_transferable(data)
    _ = ray.get(sum_matrix.remote(matrix_ref))


def main():
    data = np.ones((1000, 1000))

    with warmup_pool() as pool:
        bench_litteral_argument_value(data)
        bench_large_array_object_store(data)
        bench_pyarraypool_mp(data, pool)
        bench_pyarraypool_ray(data)


if __name__ == "__main__":
    main()
