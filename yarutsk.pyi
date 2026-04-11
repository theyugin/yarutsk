"""
Type stubs for yarutsk — a YAML round-trip library that preserves comments
and insertion order.

load / loads return YamlMapping or YamlSequence (or None for empty input).
Accessing nested nodes also returns YamlMapping or YamlSequence.
Scalar and null nodes are returned as native Python primitives.
"""

from typing import Any, Callable, IO, Iterator, overload

# The value that __getitem__ can return for a scalar leaf.
_Scalar = int | float | bool | str | None

# Any value a node accessor can return.
_Child = "YamlMapping | YamlSequence | _Scalar"

# Any top-level document node.
_Doc = "YamlMapping | YamlSequence | YamlScalar"


class YamlScalar:
    """A YAML scalar document node (int, float, bool, str, or null)."""

    @property
    def value(self) -> "_Scalar":
        """The Python primitive value of this scalar."""
        ...

    def to_dict(self) -> "_Scalar":
        """Return the Python primitive value."""
        ...

    def __eq__(self, other: object) -> bool: ...
    def __repr__(self) -> str: ...


class YamlMapping:
    """A YAML mapping node (key-value pairs)."""

    def __getitem__(self, key: str) -> "YamlMapping | YamlSequence | _Scalar": ...
    def __setitem__(self, key: str, value: "YamlMapping | YamlSequence | _Scalar") -> None: ...
    def __delitem__(self, key: str) -> None: ...
    def __contains__(self, key: str) -> bool: ...
    def __len__(self) -> int: ...
    def __iter__(self) -> Iterator[str]: ...
    def __eq__(self, other: object) -> bool: ...
    def __repr__(self) -> str: ...

    def keys(self) -> list[str]:
        """Return keys in insertion order."""
        ...

    def values(self) -> list["YamlMapping | YamlSequence | _Scalar"]:
        """Return values in insertion order."""
        ...

    def items(self) -> list[tuple[str, "YamlMapping | YamlSequence | _Scalar"]]:
        """Return (key, value) pairs in insertion order."""
        ...

    def get(
        self,
        key: str,
        default: "YamlMapping | YamlSequence | _Scalar" = None,
    ) -> "YamlMapping | YamlSequence | _Scalar":
        """Return the value for *key*, or *default* if absent."""
        ...

    def pop(
        self,
        key: str,
        default: "YamlMapping | YamlSequence | _Scalar" = None,
    ) -> "YamlMapping | YamlSequence | _Scalar":
        """Remove and return the value for *key*. Raises KeyError if absent and no default."""
        ...

    def update(self, other: "YamlMapping | dict[str, Any]") -> None:
        """Update with key-value pairs from *other*."""
        ...

    def setdefault(
        self,
        key: str,
        default: "YamlMapping | YamlSequence | _Scalar" = None,
    ) -> "YamlMapping | YamlSequence | _Scalar":
        """Return the value for *key*, inserting *default* if absent."""
        ...

    def to_dict(self) -> Any:
        """Recursively convert to a plain Python dict."""
        ...

    def get_comment_inline(self, key: str) -> str | None:
        """Return the inline comment for *key* (text after ``#``, no leading ``#``)."""
        ...

    def get_comment_before(self, key: str) -> str | None:
        """Return the block comment above *key*, lines joined with ``\\n``."""
        ...

    def set_comment_inline(self, key: str, comment: str) -> None:
        """Set or replace the inline comment for *key*."""
        ...

    def set_comment_before(self, key: str, comment: str) -> None:
        """Set or replace the block comment above *key*."""
        ...

    def sort_keys(
        self,
        key: Callable[[str], Any] | None = None,
        reverse: bool = False,
        recursive: bool = False,
    ) -> None:
        """Sort mapping keys in-place."""
        ...


class YamlSequence:
    """A YAML sequence node (ordered list)."""

    def __getitem__(self, key: int) -> "YamlMapping | YamlSequence | _Scalar": ...
    def __setitem__(self, key: int, value: "YamlMapping | YamlSequence | _Scalar") -> None: ...
    def __delitem__(self, key: int) -> None: ...
    def __contains__(self, value: "YamlMapping | YamlSequence | _Scalar") -> bool: ...
    def __len__(self) -> int: ...
    def __iter__(self) -> Iterator["YamlMapping | YamlSequence | _Scalar"]: ...
    def __eq__(self, other: object) -> bool: ...
    def __repr__(self) -> str: ...

    def to_dict(self) -> Any:
        """Recursively convert to a plain Python list."""
        ...

    def append(self, value: "YamlMapping | YamlSequence | _Scalar") -> None: ...

    def insert(self, idx: int, value: "YamlMapping | YamlSequence | _Scalar") -> None: ...

    def pop(self, idx: int = -1) -> "YamlMapping | YamlSequence | _Scalar":
        """Remove and return the item at *idx* (default last)."""
        ...

    def remove(self, value: "YamlMapping | YamlSequence | _Scalar") -> None: ...

    def extend(self, iterable: "YamlSequence | list[Any]") -> None: ...

    def index(
        self,
        value: "YamlMapping | YamlSequence | _Scalar",
        start: int = 0,
        stop: int | None = None,
    ) -> int: ...

    def count(self, value: "YamlMapping | YamlSequence | _Scalar") -> int: ...

    def get_comment_inline(self, idx: int) -> str | None:
        """Return the inline comment for the item at *idx* (text after ``#``, no leading ``#``)."""
        ...

    def get_comment_before(self, idx: int) -> str | None:
        """Return the block comment above the item at *idx*, lines joined with ``\\n``."""
        ...

    def set_comment_inline(self, idx: int, comment: str) -> None:
        """Set or replace the inline comment for the item at *idx*."""
        ...

    def set_comment_before(self, idx: int, comment: str) -> None:
        """Set or replace the block comment above the item at *idx*."""
        ...

    def reverse(self) -> None: ...

    def sort(
        self,
        key: Callable[[Any], Any] | None = None,
        reverse: bool = False,
    ) -> None:
        """Sort sequence items in-place."""
        ...


# ── Module-level functions ────────────────────────────────────────────────────

def load(stream: IO[str] | IO[bytes]) -> "YamlMapping | YamlSequence | YamlScalar | None":
    """Parse the first YAML document from a stream. Returns ``None`` for empty input."""
    ...

def loads(text: str) -> "YamlMapping | YamlSequence | YamlScalar | None":
    """Parse the first YAML document from a string. Returns ``None`` for empty input."""
    ...

def load_all(stream: IO[str] | IO[bytes]) -> "list[YamlMapping | YamlSequence | YamlScalar]":
    """Parse all YAML documents from a stream, returning a list."""
    ...

def loads_all(text: str) -> "list[YamlMapping | YamlSequence | YamlScalar]":
    """Parse all YAML documents from a string, returning a list."""
    ...

def dump(doc: "YamlMapping | YamlSequence | YamlScalar", stream: IO[str] | IO[bytes]) -> None:
    """Serialize *doc* to *stream* in block-style YAML."""
    ...

def dumps(doc: "YamlMapping | YamlSequence | YamlScalar") -> str:
    """Serialize *doc* to a YAML string."""
    ...

def dump_all(
    docs: "list[YamlMapping | YamlSequence | YamlScalar]",
    stream: IO[str] | IO[bytes],
) -> None:
    """Serialize multiple documents to *stream*, separated by ``---``."""
    ...

def dumps_all(docs: "list[YamlMapping | YamlSequence | YamlScalar]") -> str:
    """Serialize multiple documents to a string, separated by ``---``."""
    ...
