# Known issues

Here are listed few know issues and how to use workaround.

## Using multiple time same array from main process

Original source code:

```python
# in main process
x = np.array([1, 2, 3, 4])

f1 = pool.apply_async(f, pyarraypool.make_transferable(x))
f2 = pool.apply_async(f, pyarraypool.make_transferable(x))

r1, r2 = f1.get(), f2.get()
```

Sometime, you might get error when reading data from worker.

Indeed with following sequence error can occurs.

```txt

  Main                 Worker1               Worker2
-------------------------------------------------------
   |                      |                     |
   make_transferable      |                     |
   | refcount=1, flag=0   |                     |
   |                      |                     |
   |                      attach                |
   |                      | refcount=2, flag=1  |
   |                      |                     |
   |                      detach                |
   |                      | refcount=1, flag=1  |
   |                      |                     |
   make_transferable      |                     |
   | refounct=2, flag=1   |                     |
   |                      |                     |
   GC.collect()           |                     |
   | refcount=0, flag=1   |                     |
   | => release array     |                     |
   |                      |                     |
   |                      |                     attach => CRASH
```

You can fix this using following code:

```python
x = np.array([1, 2, 3, 4])
x2 = pyarraypool.make_transferable(x)

f1 = pool.apply_async(f, x2)
f2 = pool.apply_async(f, x2)

r1, r2 = f1.get(), f2.get()
```
