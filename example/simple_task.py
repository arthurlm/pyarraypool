import multiprocessing
import time

import numpy as np

import pyarraypool

# No SHM tasks ===========================================================================


def task_regular(x, value):
    # This task does not have a real intereset.
    # It is just there to show how to read / write value.
    x[:] = value
    return x


def _main_classic():
    arr = np.random.random((100, 200, 500))
    I, J, K = arr.shape

    # Init process poll
    with multiprocessing.Pool(processes=8) as pool:
        t_start = time.perf_counter()
        # Submit task to it
        results = pool.starmap(task_regular, [
            (arr[i, :, :], i) for i in range(I)
        ])

        # Assign result to local array
        for i, result in enumerate(results):
            arr[i, :, :] = result
        t_end = time.perf_counter()

        print(f"classic array duration: {(t_end - t_start) * 100.0:.3}ms")


# Pyarraypool usage ======================================================================


def task_shm(x, i, value):
    # In this task, we clearly see than object is memory mapped
    # and share between processes.
    x[i, :, :] = value


def _main_shm_manualmanagement():
    arr = np.random.random((100, 200, 500))
    I, J, K = arr.shape

    pyarraypool.configure_global_pool(autostart=False)

    # Init process pool
    # NB. set initializer to make it able to receive data from SHM.
    #
    # Init memory array pool
    with multiprocessing.Pool(processes=8, initializer=pyarraypool.start_pool) as pool, \
            pyarraypool.object_pool_context():
        t_start = time.perf_counter()
        # Transfer the array to shared memory
        shmarr = pyarraypool.make_transferable(arr)

        # Apply task to array
        pool.starmap(task_shm, [
            (shmarr, i, i) for i in range(I)
        ])
        t_end = time.perf_counter()

        print(f"shm array duration: {(t_end - t_start) * 100.0:.3}ms")


def _main_shm_autostart():
    arr = np.random.random((100, 200, 500))
    I, J, K = arr.shape

    pyarraypool.configure_global_pool(autostart=True)

    # Init process pool
    with multiprocessing.Pool(processes=8) as pool:
        t_start = time.perf_counter()
        # Transfer the array to shared memory.
        #
        # Pool will be started automatically on first call.
        shmarr = pyarraypool.make_transferable(arr)

        # Apply task to array
        pool.starmap(task_shm, [
            (shmarr, i, i) for i in range(I)
        ])
        t_end = time.perf_counter()

        print(f"shm array duration: {(t_end - t_start) * 100.0:.3}ms")


if __name__ == "__main__":
    # Clear any previous SHM if found
    pyarraypool.cleanup_shm()

    # Run main
    _main_classic()
    _main_shm_manualmanagement()
    _main_shm_autostart()
