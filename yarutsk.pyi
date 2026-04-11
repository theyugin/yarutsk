"""
Type stubs for yarutsk — a YAML round-trip library that preserves comments
and insertion order.

load / loads return YamlMapping (a dict subclass) or YamlSequence (a list
subclass), or None for empty input. Accessing nested nodes returns the same
types. Scalar leaves and null values are returned as native Python primitives.
"""

from typing import Any, Callable, IO

# The value that __getitem__ can return for a scalar leaf.
_Scalar = int | float | bool | str | None

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

class YamlMapping(dict[str, Any]):
    """A YAML mapping node. Subclass of dict — all standard dict operations work.

    In addition to the full dict interface, provides:
    - Comment access/mutation methods
    - ``sort_keys()`` for in-place key sorting
    - ``to_dict()`` for deep conversion to a plain Python dict
    """

    def __repr__(self) -> str: ...
    def sort_keys(
        self,
        key: Callable[[str], Any] | None = None,
        reverse: bool = False,
        recursive: bool = False,
    ) -> None:
        """Sort mapping keys in-place."""
        ...

    def to_dict(self) -> Any:
        """Recursively convert to a plain Python dict (no YamlMapping/YamlSequence nodes)."""
        ...

    def get_comment_inline(self, key: str) -> str | None:
        """Return the inline comment for *key* (text after ``#``, no leading ``#``)."""
        ...

    def get_comment_before(self, key: str) -> str | None:
        """Return the block comment above *key*, lines joined with ``\\n``."""
        ...

    def set_comment_inline(self, key: str, comment: str | None) -> None:
        """Set or replace the inline comment for *key*."""
        ...

    def set_comment_before(self, key: str, comment: str | None) -> None:
        """Set or replace the block comment above *key*."""
        ...

class YamlSequence(list[Any]):
    """A YAML sequence node. Subclass of list — all standard list operations work.

    In addition to the full list interface, provides:
    - Comment access/mutation methods (addressed by integer index)
    - ``sort()`` override that preserves comment metadata
    - ``to_dict()`` for deep conversion to a plain Python list
    """

    def __repr__(self) -> str: ...
    def sort(
        self,
        key: Callable[[Any], Any] | None = None,
        reverse: bool = False,
    ) -> None:
        """Sort sequence items in-place, preserving comment metadata."""
        ...

    def to_dict(self) -> Any:
        """Recursively convert to a plain Python list (no YamlMapping/YamlSequence nodes)."""
        ...

    def get_comment_inline(self, idx: int) -> str | None:
        """Return the inline comment for the item at *idx* (text after ``#``, no leading ``#``)."""
        ...

    def get_comment_before(self, idx: int) -> str | None:
        """Return the block comment above the item at *idx*, lines joined with ``\\n``."""
        ...

    def set_comment_inline(self, idx: int, comment: str | None) -> None:
        """Set or replace the inline comment for the item at *idx*."""
        ...

    def set_comment_before(self, idx: int, comment: str | None) -> None:
        """Set or replace the block comment above the item at *idx*."""
        ...

# ── Module-level functions ────────────────────────────────────────────────────

def load(
    stream: IO[str] | IO[bytes],
) -> "YamlMapping | YamlSequence | YamlScalar | None":
    """Parse the first YAML document from a stream. Returns ``None`` for empty input."""
    ...

def loads(text: str) -> "YamlMapping | YamlSequence | YamlScalar | None":
    """Parse the first YAML document from a string. Returns ``None`` for empty input."""
    ...

def load_all(
    stream: IO[str] | IO[bytes],
) -> "list[YamlMapping | YamlSequence | YamlScalar]":
    """Parse all YAML documents from a stream, returning a list."""
    ...

def loads_all(text: str) -> "list[YamlMapping | YamlSequence | YamlScalar]":
    """Parse all YAML documents from a string, returning a list."""
    ...

def dump(
    doc: "YamlMapping | YamlSequence | YamlScalar", stream: IO[str] | IO[bytes]
) -> None:
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
