"""
Type stubs for yarutsk — a YAML round-trip library that preserves comments
and insertion order.

load / loads return YamlMapping (a dict subclass) or YamlSequence (a list
subclass), or None for empty input. Accessing nested nodes returns the same
types. Scalar leaves and null values are returned as native Python primitives.
"""

from typing import Any, Callable, IO, SupportsIndex, TypeVar, overload

_T = TypeVar("_T")

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

    @property
    def style(self) -> str:
        """The scalar quoting style: ``"plain"``, ``"single"``, ``"double"``, ``"literal"``, or ``"folded"``."""
        ...

    @style.setter
    def style(self, value: str) -> None: ...
    @property
    def explicit_start(self) -> bool:
        """Whether the source document had an explicit ``---`` marker."""
        ...

    @explicit_start.setter
    def explicit_start(self, value: bool) -> None: ...
    @property
    def explicit_end(self) -> bool:
        """Whether the source document had an explicit ``...`` marker."""
        ...

    @explicit_end.setter
    def explicit_end(self, value: bool) -> None: ...
    @property
    def tag(self) -> str | None:
        """The YAML tag on this scalar (e.g. ``"!!str"``), or ``None``."""
        ...

    @tag.setter
    def tag(self, value: str | None) -> None: ...
    @property
    def yaml_version(self) -> str | None:
        """The ``%YAML`` version directive for this document (e.g. ``"1.2"``), or ``None``."""
        ...

    @yaml_version.setter
    def yaml_version(self, value: str | None) -> None: ...
    @property
    def tag_directives(self) -> list[tuple[str, str]]:
        """The ``%TAG`` directives for this document as a list of ``(handle, prefix)`` pairs."""
        ...

    @tag_directives.setter
    def tag_directives(self, value: list[tuple[str, str]]) -> None: ...
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

    @property
    def explicit_start(self) -> bool:
        """Whether the source document had an explicit ``---`` marker."""
        ...

    @explicit_start.setter
    def explicit_start(self, value: bool) -> None: ...
    @property
    def explicit_end(self) -> bool:
        """Whether the source document had an explicit ``...`` marker."""
        ...

    @explicit_end.setter
    def explicit_end(self, value: bool) -> None: ...
    @property
    def tag(self) -> str | None:
        """The YAML tag on this mapping (e.g. ``"!!map"``), or ``None``."""
        ...

    @tag.setter
    def tag(self, value: str | None) -> None: ...
    @property
    def yaml_version(self) -> str | None:
        """The ``%YAML`` version directive for this document (e.g. ``"1.2"``), or ``None``."""
        ...

    @yaml_version.setter
    def yaml_version(self, value: str | None) -> None: ...
    @property
    def tag_directives(self) -> list[tuple[str, str]]:
        """The ``%TAG`` directives for this document as a list of ``(handle, prefix)`` pairs."""
        ...

    @tag_directives.setter
    def tag_directives(self, value: list[tuple[str, str]]) -> None: ...
    def node(self, key: str) -> "YamlMapping | YamlSequence | YamlScalar":
        """Return the underlying YAML node for *key*, preserving style/tag metadata.
        Raises ``KeyError`` if *key* is absent.
        """
        ...

    def scalar_style(self, key: str, style: str) -> None:
        """Set the scalar quoting style for the value at *key*.
        *style* must be one of ``"plain"``, ``"single"``, ``"double"``, ``"literal"``, ``"folded"``.
        Raises ``KeyError`` if *key* is absent; ``ValueError`` for unknown styles.
        """
        ...

    def clear(self) -> None:
        """Remove all entries from this mapping."""
        ...

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

    @overload
    def comment_inline(self, key: str) -> str | None:
        """Return the inline comment for *key* (text after ``#``, no leading ``#``), or ``None``.
        Raises ``KeyError`` if *key* is absent.
        """
        ...

    @overload
    def comment_inline(self, key: str, comment: str | None) -> None:
        """Set or clear the inline comment for *key*. Pass ``None`` to remove it.
        Raises ``KeyError`` if *key* is absent.
        """
        ...

    @overload
    def comment_before(self, key: str) -> str | None:
        """Return the block comment above *key*, lines joined with ``\\n``, or ``None``.
        Raises ``KeyError`` if *key* is absent.
        """
        ...

    @overload
    def comment_before(self, key: str, comment: str | None) -> None:
        """Set or clear the block comment above *key*. Pass ``None`` to remove it.
        Raises ``KeyError`` if *key* is absent.
        """
        ...

class YamlSequence(list[Any]):
    """A YAML sequence node. Subclass of list — all standard list operations work.

    In addition to the full list interface, provides:
    - Comment access/mutation methods (addressed by integer index)
    - ``sort()`` override that preserves comment metadata
    - ``to_dict()`` for deep conversion to a plain Python list
    """

    @property
    def explicit_start(self) -> bool:
        """Whether the source document had an explicit ``---`` marker."""
        ...

    @explicit_start.setter
    def explicit_start(self, value: bool) -> None: ...
    @property
    def explicit_end(self) -> bool:
        """Whether the source document had an explicit ``...`` marker."""
        ...

    @explicit_end.setter
    def explicit_end(self, value: bool) -> None: ...
    @property
    def tag(self) -> str | None:
        """The YAML tag on this sequence (e.g. ``"!!seq"``), or ``None``."""
        ...

    @tag.setter
    def tag(self, value: str | None) -> None: ...
    @property
    def yaml_version(self) -> str | None:
        """The ``%YAML`` version directive for this document (e.g. ``"1.2"``), or ``None``."""
        ...

    @yaml_version.setter
    def yaml_version(self, value: str | None) -> None: ...
    @property
    def tag_directives(self) -> list[tuple[str, str]]:
        """The ``%TAG`` directives for this document as a list of ``(handle, prefix)`` pairs."""
        ...

    @tag_directives.setter
    def tag_directives(self, value: list[tuple[str, str]]) -> None: ...
    def clear(self) -> None:
        """Remove all items from this sequence."""
        ...

    def index(
        self, value: object, start: SupportsIndex = ..., stop: SupportsIndex = ...
    ) -> int:
        """Return the index of the first occurrence of *value*."""
        ...

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

    @overload
    def comment_inline(self, idx: int) -> str | None:
        """Return the inline comment for the item at *idx* (text after ``#``, no leading ``#``), or ``None``."""
        ...

    @overload
    def comment_inline(self, idx: int, comment: str | None) -> None:
        """Set or clear the inline comment for the item at *idx*. Pass ``None`` to remove it."""
        ...

    @overload
    def comment_before(self, idx: int) -> str | None:
        """Return the block comment above the item at *idx*, lines joined with ``\\n``, or ``None``."""
        ...

    @overload
    def comment_before(self, idx: int, comment: str | None) -> None:
        """Set or clear the block comment above the item at *idx*. Pass ``None`` to remove it."""
        ...

class Schema:
    """A per-call type registry for customising load and dump behaviour.

    Pass a ``Schema`` instance as the ``schema=`` keyword argument to any
    load or dump function. It carries:

    - **loaders** — tag → callable, called during load when a node carries that tag
    - **dumpers** — type → callable, called during dump for matching Python objects

    Example::

        schema = yarutsk.Schema()
        schema.add_loader("!point", lambda d: Point(d["x"], d["y"]))
        schema.add_dumper(Point, lambda p: ("!point", {"x": p.x, "y": p.y}))

        doc = yarutsk.loads(src, schema=schema)
        out = yarutsk.dumps(doc, schema=schema)
    """

    def __init__(self) -> None: ...
    def add_loader(self, tag: str, func: Callable[[Any], Any]) -> None:
        """Register a loader callable for *tag*.

        The callable receives the default-converted Python value:

        - For scalar nodes: ``str``, ``int``, ``float``, ``bool``, or ``None``
          (for built-in coercion tags such as ``!!int`` / ``!!bool`` / ``!!null`` /
          ``!!float`` / ``!!str``, the builder is bypassed and the raw YAML string
          is passed instead, giving full control over parsing)
        - For mapping nodes: a ``YamlMapping``
        - For sequence nodes: a ``YamlSequence``

        The return value replaces the node in the loaded document.
        """
        ...

    def add_dumper(
        self, py_type: type[_T], func: Callable[[_T], tuple[str, Any]]
    ) -> None:
        """Register a dumper callable for *py_type*.

        Dumpers are checked in registration order; the first ``isinstance`` match
        wins. The callable receives the Python object and must return a 2-tuple
        ``(tag: str, data)``, where *data* is a scalar, dict, or list that will
        be serialized as the node body.
        """
        ...

# ── Module-level functions ────────────────────────────────────────────────────

def load(
    stream: IO[str] | IO[bytes],
    *,
    schema: Schema | None = None,
) -> "YamlMapping | YamlSequence | YamlScalar | None":
    """Parse the first YAML document from a stream. Returns ``None`` for empty input."""
    ...

def loads(
    text: str,
    *,
    schema: Schema | None = None,
) -> "YamlMapping | YamlSequence | YamlScalar | None":
    """Parse the first YAML document from a string. Returns ``None`` for empty input."""
    ...

def load_all(
    stream: IO[str] | IO[bytes],
    *,
    schema: Schema | None = None,
) -> "list[YamlMapping | YamlSequence | YamlScalar]":
    """Parse all YAML documents from a stream, returning a list."""
    ...

def loads_all(
    text: str,
    *,
    schema: Schema | None = None,
) -> "list[YamlMapping | YamlSequence | YamlScalar]":
    """Parse all YAML documents from a string, returning a list."""
    ...

def dump(
    doc: "YamlMapping | YamlSequence | YamlScalar",
    stream: IO[str] | IO[bytes],
    *,
    schema: Schema | None = None,
) -> None:
    """Serialize *doc* to *stream* in block-style YAML."""
    ...

def dumps(
    doc: "YamlMapping | YamlSequence | YamlScalar",
    *,
    schema: Schema | None = None,
) -> str:
    """Serialize *doc* to a YAML string."""
    ...

def dump_all(
    docs: "list[YamlMapping | YamlSequence | YamlScalar]",
    stream: IO[str] | IO[bytes],
    *,
    schema: Schema | None = None,
) -> None:
    """Serialize multiple documents to *stream*, separated by ``---``."""
    ...

def dumps_all(
    docs: "list[YamlMapping | YamlSequence | YamlScalar]",
    *,
    schema: Schema | None = None,
) -> str:
    """Serialize multiple documents to a string, separated by ``---``."""
    ...
