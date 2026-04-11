"""
Type stubs for yarutsk — a YAML round-trip library that preserves comments
and insertion order.

YamlDocument is the top-level wrapper returned by load / loads.
Accessing nested nodes returns YamlMapping or YamlSequence.
Scalar and null nodes are returned as native Python primitives.
"""

from typing import Any, Callable, IO, overload

# The value that __getitem__ can return for a scalar leaf.
_Scalar = int | float | bool | str | None

# The value that __getitem__ can return for any child node.
_Child = "YamlMapping | YamlSequence | _Scalar"


class YamlDocument:
    """Top-level YAML document returned by load / loads.

    Proxies all operations to the root node (mapping or sequence).
    """

    @overload
    def __getitem__(self, key: str) -> "YamlMapping | YamlSequence | _Scalar": ...
    @overload
    def __getitem__(self, key: int) -> "YamlMapping | YamlSequence | _Scalar": ...

    @overload
    def __setitem__(self, key: str, value: "YamlMapping | YamlSequence | _Scalar") -> None: ...
    @overload
    def __setitem__(self, key: int, value: "YamlMapping | YamlSequence | _Scalar") -> None: ...

    def __contains__(self, key: str) -> bool: ...
    def __len__(self) -> int: ...
    def __repr__(self) -> str: ...

    def keys(self) -> list[str]:
        """Return mapping keys in insertion order."""
        ...

    def to_dict(self) -> Any:
        """Recursively convert to plain Python dicts, lists, and primitives."""
        ...

    def get_comment_inline(self, key: str) -> str | None: ...
    def get_comment_before(self, key: str) -> str | None: ...
    def set_comment_inline(self, key: str, comment: str) -> None: ...
    def set_comment_before(self, key: str, comment: str) -> None: ...

    def sort_keys(
        self,
        key: Callable[[str], Any] | None = None,
        reverse: bool = False,
        recursive: bool = False,
    ) -> None: ...

    def sort(
        self,
        key: Callable[[Any], Any] | None = None,
        reverse: bool = False,
    ) -> None: ...


class YamlMapping:
    """A YAML mapping node (key-value pairs), returned when accessing a nested mapping."""

    def __getitem__(self, key: str) -> "YamlMapping | YamlSequence | _Scalar": ...
    def __setitem__(self, key: str, value: "YamlMapping | YamlSequence | _Scalar") -> None: ...
    def __contains__(self, key: str) -> bool: ...
    def __len__(self) -> int: ...
    def __repr__(self) -> str: ...

    def keys(self) -> list[str]:
        """Return keys in insertion order."""
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
    """A YAML sequence node (ordered list), returned when accessing a nested sequence."""

    def __getitem__(self, key: int) -> "YamlMapping | YamlSequence | _Scalar": ...
    def __setitem__(self, key: int, value: "YamlMapping | YamlSequence | _Scalar") -> None: ...
    def __len__(self) -> int: ...
    def __repr__(self) -> str: ...

    def to_dict(self) -> Any:
        """Recursively convert to a plain Python list."""
        ...

    def sort(
        self,
        key: Callable[[Any], Any] | None = None,
        reverse: bool = False,
    ) -> None:
        """Sort sequence items in-place."""
        ...


# ── Module-level functions ────────────────────────────────────────────────────

def load(stream: IO[str] | IO[bytes]) -> YamlDocument | None:
    """Parse the first YAML document from a stream. Returns ``None`` for empty input."""
    ...

def loads(text: str) -> YamlDocument | None:
    """Parse the first YAML document from a string. Returns ``None`` for empty input."""
    ...

def load_all(stream: IO[str] | IO[bytes]) -> list[YamlDocument]:
    """Parse all YAML documents from a stream, returning a list."""
    ...

def loads_all(text: str) -> list[YamlDocument]:
    """Parse all YAML documents from a string, returning a list."""
    ...

def dump(doc: YamlDocument, stream: IO[str] | IO[bytes]) -> None:
    """Serialize *doc* to *stream* in block-style YAML."""
    ...

def dumps(doc: YamlDocument) -> str:
    """Serialize *doc* to a YAML string."""
    ...

def dump_all(docs: list[YamlDocument], stream: IO[str] | IO[bytes]) -> None:
    """Serialize multiple documents to *stream*, separated by ``---``."""
    ...

def dumps_all(docs: list[YamlDocument]) -> str:
    """Serialize multiple documents to a string, separated by ``---``."""
    ...
