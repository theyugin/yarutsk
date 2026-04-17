# Thread safety

All top-level functions (`load`, `loads`, `load_all`, `loads_all`, `iter_load_all`, `iter_loads_all`, `dump`, `dumps`, `dump_all`, `dumps_all`) are safe to call concurrently from multiple Python threads. Each call owns its parser, builder, and emitter state — there is no shared mutable global.

A single `Schema` instance can be shared read-only across threads as long as you do not call `add_loader` / `add_dumper` on it while other threads are in flight. The registration API is not itself internally synchronised.

Individual `YamlMapping` / `YamlSequence` / `YamlScalar` instances follow the same rule as plain Python `dict` / `list`: they are not protected against concurrent mutation from multiple threads. Either scope one instance per thread or serialise access externally.

The [`tests/test_threading.py`](https://github.com/theyugin/yarutsk/blob/main/tests/test_threading.py) suite exercises 30 concurrent load/dump scenarios under the GIL and on free-threaded Python.
