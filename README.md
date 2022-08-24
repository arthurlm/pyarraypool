# Pyarraypool

![Licence](https://img.shields.io/github/license/arthurlm/pyarraypool)
![Python build](https://img.shields.io/github/workflow/status/arthurlm/pyarraypool/Python?label=build%20python)
![Rust build](https://img.shields.io/github/workflow/status/arthurlm/pyarraypool/Rust?label=build%20rust)

Transfer numpy array between processes using shared memory.

## Why creating this project ?

This library aims to speed up parallel data processing with python and numpy NDArray.

Python GIL does not permit to use multithreading for parallel data processing.
It is indeed release when C code / Cython / IO tasks are done but it is still lock for computation tasks.

Alternative to subprocess worker exists but they are not always possible to use.
To list few of them:

- numba
- switching from [cpython](https://github.com/python/cpython) to [pypy](https://www.pypy.org/)
- rewrite code using C / Cython / Rust

## Few design choices

Python standard library already contains a module to create and manage [shared memory](https://docs.python.org/3/library/multiprocessing.shared_memory.html).

However it does not permit to manage it as a raw bloc.
So performances drop because several system call must be done on each bloc creation / deletion.

In this library:

- shared memory is manage as a "pool".
- array can be attached and are release when refcount reach 0 in every processes.
- a spinlock is used to manage sync between process when bloc are add / removed (this can be improved).

## API usage

Here a simple example of how to use library.

```python
import pyarraypool
import multiprocessing
import numpy as np

def task(x, i, value):
    # Define a dummy task
    x[i, :, :] = value

def main():
    arr = np.random.random((100, 200, 500))
    I, J, K = arr.shape

    with multiprocessing.get_context("spawn").Pool(processes=8, initializer=pyarraypool.start_pool) as pool, \
            pyarraypool.object_pool():
        # Transfer the array to shared memory
        shmarr = pyarraypool.make_transferable(arr)

        # Apply task to array
        pool.starmap(task, [
            (shmarr, i, i) for i in range(I)
        ])

if __name__ == "__main__":
    main()
```

You can have a look at `notebook` / `example` folders for more details.

## Developper guide

To build:

```sh
pip install maturin
maturin develop --extras test
```

To test:

```sh
# Run rust tests
cargo test
cargo clippy

# Run python tests
pytest -vv
flake8
autopep8 --diff -r python/
mypy .
```

To format code:

```
autopep8 -ir python/
isort .
```

## Project status

Project is currently a "POC" and not fully ready for production.

Few benchmark are still missing.
API can be improved.

See `TODO.md` for more details.

Any help / feedback is welcome 😊 !
