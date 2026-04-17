"""
Type stubs for yarutsk — a YAML round-trip library that preserves comments
and insertion order.

load / loads return YamlMapping (a dict subclass) or YamlSequence (a list
subclass), or None for empty input. Accessing nested nodes returns the same
types. Scalar leaves and null values are returned as native Python primitives.
"""

from collections.abc import Mapping as _Mapping, Sequence as _Sequence
from typing import (
    Any,
    Callable,
    IO,
    Iterable,
    Literal,
    SupportsIndex,
    TypeAlias,
    TypeVar,
    overload,
)

_T = TypeVar("_T")

# The value that __getitem__ can return for a scalar leaf.
_Scalar = int | float | bool | str | None

# Any top-level document node.
_Doc = "YamlMapping | YamlSequence | YamlScalar"

# Any value accepted by dump/dumps — the Yaml types, abstract containers,
# bytes, and scalar primitives.
_Dumpable: TypeAlias = "YamlMapping | YamlSequence | YamlScalar | _Mapping[str, Any] | Iterable[Any] | bytes | bytearray | int | float | bool | str | None"

# ── Public type aliases ───────────────────────────────────────────────────────

ScalarStyle: TypeAlias = Literal["plain", "single", "double", "literal", "folded"]
"""The quoting style of a YAML scalar."""

ContainerStyle: TypeAlias = Literal["block", "flow"]
"""The layout style of a YAML mapping or sequence."""

YamlNode: TypeAlias = "YamlMapping | YamlSequence | YamlScalar"
"""Any YAML document node (mapping, sequence, or scalar)."""

class YarutskError(Exception):
    """Base exception for all yarutsk errors."""

    ...

class ParseError(YarutskError):
    """Raised when the YAML input cannot be parsed.

    The message includes the error description plus byte offset, line, and column
    from the scanner (e.g. ``"did not find expected key at byte 10 line 3 column 1"``).
    """

    ...

class LoaderError(YarutskError):
    """Raised when a schema loader callable raises an exception.

    The message includes the tag name and the original exception, e.g.:
    ``"Schema loader for tag '!color' raised: AttributeError: ..."``
    """

    ...

class DumperError(YarutskError):
    """Raised when a schema dumper callable raises an exception, or when it
    returns something other than a ``(tag, data)`` tuple.

    The message includes the Python type name and the original exception.
    """

    ...

class YamlScalar:
    """A YAML scalar document node (int, float, bool, str, or null).

    Can be constructed directly to create a styled scalar for assignment or
    use in a Schema dumper::

        doc["key"] = yarutsk.YamlScalar("hello", style="double")
        schema.add_dumper(MyType, lambda obj: ("!mytag", yarutsk.YamlScalar(str(obj), style="single")))
    """

    def __init__(
        self,
        value: "_Scalar",
        *,
        style: "ScalarStyle" = "plain",
        tag: str | None = None,
    ) -> None:
        """Create a scalar with the given primitive value, quoting style, and optional tag.

        *style* must be one of ``"plain"``, ``"single"``, ``"double"``, ``"literal"``, ``"folded"``.
        Raises ``TypeError`` if *value* is not a Python primitive.
        Raises ``ValueError`` for an unknown *style*.
        """
        ...

    @property
    def value(self) -> "_Scalar":
        """The Python primitive value of this scalar."""
        ...

    @property
    def style(self) -> "ScalarStyle":
        """The scalar quoting style: ``"plain"``, ``"single"``, ``"double"``, ``"literal"``, or ``"folded"``."""
        ...

    @style.setter
    def style(self, value: "ScalarStyle") -> None: ...
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
    def anchor(self) -> str | None:
        """The anchor name declared on this scalar (``&name``), or ``None``."""
        ...

    @anchor.setter
    def anchor(self, value: str | None) -> None: ...
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
    def format(
        self,
        *,
        styles: bool = True,
        comments: bool = True,
        blank_lines: bool = True,
    ) -> None:
        """Strip cosmetic formatting, resetting to clean YAML defaults.

        ``styles``: scalar quoting → plain (literal for multi-line strings),
        ``original`` cleared so non-canonical forms emit canonically.
        ``comments`` and ``blank_lines`` are accepted for API consistency but are
        no-ops on scalars. Tags and anchors are always preserved.
        """
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

    Can be constructed directly to create a styled mapping::

        m = yarutsk.YamlMapping(style="flow")
        m["x"] = 1
        m["y"] = 2
        doc["point"] = m
    """

    def __init__(
        self,
        mapping: "dict[str, Any] | YamlMapping | None" = None,
        *,
        style: Literal["block", "flow"] = "block",
        tag: str | None = None,
    ) -> None:
        """Create a mapping, optionally populated from *mapping*.

        If *mapping* is a ``YamlMapping``, inner metadata (comments, styles,
        anchors) is preserved. If it is a plain ``dict`` or another mapping, it
        is iterated and entries are set as plain values. Raises ``ValueError``
        for an unknown *style*.
        """
        ...

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
    def anchor(self) -> str | None:
        """The anchor name declared on this mapping (``&name``), or ``None``."""
        ...

    @anchor.setter
    def anchor(self, value: str | None) -> None: ...
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
    @property
    def style(self) -> Literal["block", "flow"]:
        """The container style: ``"block"`` (default) or ``"flow"`` (inline ``{…}``)."""
        ...

    @style.setter
    def style(self, value: Literal["block", "flow"]) -> None: ...
    @property
    def trailing_blank_lines(self) -> int:
        """Number of blank lines emitted after the last entry in this mapping (0–255)."""
        ...

    @trailing_blank_lines.setter
    def trailing_blank_lines(self, value: int) -> None: ...
    def node(self, key: str) -> "YamlMapping | YamlSequence | YamlScalar":
        """Return the underlying YAML node for *key*, preserving style/tag metadata.
        Raises ``KeyError`` if *key* is absent.
        """
        ...

    def scalar_style(self, key: str, style: "ScalarStyle") -> None:
        """Set the scalar quoting style for the value at *key*.
        *style* must be one of ``"plain"``, ``"single"``, ``"double"``, ``"literal"``, ``"folded"``.
        Raises ``KeyError`` if *key* is absent; ``ValueError`` for unknown styles;
        ``TypeError`` if the value is not a scalar (use ``container_style()`` instead).
        """
        ...

    def container_style(self, key: str, style: Literal["block", "flow"]) -> None:
        """Set the container style for the nested mapping or sequence at *key*.
        *style* must be ``"block"`` or ``"flow"``.
        Raises ``KeyError`` if *key* is absent; ``ValueError`` for unknown styles.
        Silently ignored if the value at *key* is not a mapping or sequence.
        """
        ...

    @overload
    def blank_lines_before(self, key: str) -> int:
        """Return the number of blank lines before *key* (0 if none).
        Raises ``KeyError`` if *key* is absent.
        """
        ...

    @overload
    def blank_lines_before(self, key: str, n: int) -> None:
        """Set the number of blank lines before *key*. Values are clamped to 0–255.
        Raises ``KeyError`` if *key* is absent.
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

    def get_comment_inline(self, key: str) -> str | None:
        """Return the inline comment for *key*, or ``None`` if unset.
        Raises ``KeyError`` if *key* is absent.
        """
        ...

    def set_comment_inline(self, key: str, comment: str | None) -> None:
        """Set the inline comment for *key*; pass ``None`` to clear.
        Raises ``KeyError`` if *key* is absent.
        """
        ...

    def get_comment_before(self, key: str) -> str | None:
        """Return the block comment above *key*, lines joined with ``\\n``, or ``None`` if unset.
        Raises ``KeyError`` if *key* is absent.
        """
        ...

    def set_comment_before(self, key: str, comment: str | None) -> None:
        """Set the block comment above *key*; pass ``None`` to clear.
        Raises ``KeyError`` if *key* is absent.
        """
        ...

    def alias_name(self, key: str) -> str | None:
        """Return the anchor name if the value at *key* is a YAML alias node, else ``None``.

        A value is an alias node when it was parsed from ``*name`` YAML syntax or when
        ``set_alias()`` was called on it.  The resolved Python value is still accessible
        via normal ``__getitem__``.  Raises ``KeyError`` if *key* is absent.
        """
        ...

    def set_alias(self, key: str, anchor_name: str) -> None:
        """Mark the value at *key* as emitting ``*anchor_name`` in YAML output.

        The current resolved value is retained so Python reads (``doc[key]``) keep
        working. Raises ``KeyError`` if *key* is absent.
        """
        ...

    def format(
        self,
        *,
        styles: bool = True,
        comments: bool = True,
        blank_lines: bool = True,
    ) -> None:
        """Strip cosmetic formatting metadata, resetting to clean YAML defaults.

        ``styles``: scalar quoting → plain (or ``literal`` for multi-line strings),
        container style → block, ``original`` values cleared.
        ``comments``: ``comment_before`` and ``comment_inline`` cleared on all entries.
        ``blank_lines``: ``blank_lines_before`` zeroed; ``trailing_blank_lines`` zeroed.
        Tags, anchors, and document-level markers are always preserved.
        Recurses into all nested containers.
        """
        ...

    @classmethod
    def from_dict(
        cls,
        obj: "dict[str, Any]",
        *,
        schema: "Schema | None" = None,
    ) -> "YamlMapping":
        """Create a ``YamlMapping`` from a plain Python dict (or any dict-like object).

        Nested dicts are recursively converted to ``YamlMapping``, nested lists to
        ``YamlSequence``. Raises ``TypeError`` if *obj* is not dict-like.
        """
        ...

    def nodes(self) -> "list[tuple[str, YamlMapping | YamlSequence | YamlScalar]]":
        """Return a list of ``(key, node)`` pairs for all entries in this mapping.

        Each node is a ``YamlMapping``, ``YamlSequence``, or ``YamlScalar``,
        preserving style/tag metadata. Unlike ``items()``, which returns plain
        Python values, ``nodes()`` returns the full typed node objects.
        """
        ...

    def copy(self) -> "YamlMapping": ...
    def __copy__(self) -> "YamlMapping": ...
    def __deepcopy__(self, memo: "dict[int, Any]") -> "YamlMapping": ...

class YamlSequence(list[Any]):
    """A YAML sequence node. Subclass of list — all standard list operations work.

    In addition to the full list interface, provides:
    - Comment access/mutation methods (addressed by integer index)
    - ``sort()`` override that preserves comment metadata
    - ``to_dict()`` for deep conversion to a plain Python list

    Can be constructed directly to create a styled sequence::

        s = yarutsk.YamlSequence(style="flow")
        s.extend([1, 2, 3])
        doc["items"] = s
    """

    def __init__(
        self,
        iterable: "Iterable[Any] | None" = None,
        *,
        style: Literal["block", "flow"] = "block",
        tag: str | None = None,
    ) -> None:
        """Create a sequence, optionally populated from *iterable*.

        If *iterable* is a ``YamlSequence``, inner metadata (comments, styles,
        anchors) is preserved. Any other iterable has its items appended as plain
        values. Raises ``ValueError`` for an unknown *style*.
        """
        ...

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
    def anchor(self) -> str | None:
        """The anchor name declared on this sequence (``&name``), or ``None``."""
        ...

    @anchor.setter
    def anchor(self, value: str | None) -> None: ...
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
    @property
    def style(self) -> Literal["block", "flow"]:
        """The container style: ``"block"`` (default) or ``"flow"`` (inline ``[…]``)."""
        ...

    @style.setter
    def style(self, value: Literal["block", "flow"]) -> None: ...
    @property
    def trailing_blank_lines(self) -> int:
        """Number of blank lines emitted after the last item in this sequence (0–255)."""
        ...

    @trailing_blank_lines.setter
    def trailing_blank_lines(self, value: int) -> None: ...
    def scalar_style(self, idx: int, style: "ScalarStyle") -> None:
        """Set the scalar quoting style for the item at *idx*.
        *style* must be one of ``"plain"``, ``"single"``, ``"double"``, ``"literal"``, ``"folded"``.
        Raises ``IndexError`` for out-of-range indices; ``ValueError`` for unknown styles;
        ``TypeError`` if the item is not a scalar (use ``container_style()`` instead).
        """
        ...

    def container_style(self, idx: int, style: Literal["block", "flow"]) -> None:
        """Set the container style for the nested mapping or sequence at *idx*.
        *style* must be ``"block"`` or ``"flow"``.
        Raises ``IndexError`` for out-of-range indices; ``ValueError`` for unknown styles.
        Silently ignored if the item at *idx* is not a mapping or sequence.
        """
        ...

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
        recursive: bool = False,
    ) -> None:
        """Sort sequence items in-place, preserving comment metadata.

        When *recursive* is ``True``, any nested ``YamlMapping`` values have their
        keys sorted by natural string order (``sort_keys`` with no *key* function),
        and any nested ``YamlSequence`` values are sorted recursively with the same
        *key* and *reverse* arguments.
        """
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

    def get_comment_inline(self, idx: int) -> str | None:
        """Return the inline comment for the item at *idx*, or ``None`` if unset.
        Raises ``IndexError`` for out-of-range indices.
        """
        ...

    def set_comment_inline(self, idx: int, comment: str | None) -> None:
        """Set the inline comment for the item at *idx*; pass ``None`` to clear.
        Raises ``IndexError`` for out-of-range indices.
        """
        ...

    def get_comment_before(self, idx: int) -> str | None:
        """Return the block comment above the item at *idx*, lines joined with ``\\n``, or ``None`` if unset.
        Raises ``IndexError`` for out-of-range indices.
        """
        ...

    def set_comment_before(self, idx: int, comment: str | None) -> None:
        """Set the block comment above the item at *idx*; pass ``None`` to clear.
        Raises ``IndexError`` for out-of-range indices.
        """
        ...

    def alias_name(self, idx: int) -> str | None:
        """Return the anchor name if the item at *idx* is a YAML alias node, else ``None``.

        A value is an alias node when it was parsed from ``*name`` YAML syntax or when
        ``set_alias()`` was called on it.  The resolved Python value is still accessible
        via normal ``__getitem__``.  Raises ``IndexError`` for out-of-range indices.
        """
        ...

    def set_alias(self, idx: int, anchor_name: str) -> None:
        """Mark the item at *idx* as emitting ``*anchor_name`` in YAML output.

        The current resolved value is retained so Python reads (``seq[idx]``) keep
        working. Raises ``IndexError`` for out-of-range indices.
        """
        ...

    @overload
    def blank_lines_before(self, idx: int) -> int:
        """Return the number of blank lines before the item at *idx* (0 if none)."""
        ...

    @overload
    def blank_lines_before(self, idx: int, n: int) -> None:
        """Set the number of blank lines before the item at *idx*. Values are clamped to 0–255."""
        ...

    def format(
        self,
        *,
        styles: bool = True,
        comments: bool = True,
        blank_lines: bool = True,
    ) -> None:
        """Strip cosmetic formatting metadata, resetting to clean YAML defaults.

        ``styles``: scalar quoting → plain (or ``literal`` for multi-line strings),
        container style → block, ``original`` values cleared.
        ``comments``: ``comment_before`` and ``comment_inline`` cleared on all items.
        ``blank_lines``: ``blank_lines_before`` zeroed; ``trailing_blank_lines`` zeroed.
        Tags, anchors, and document-level markers are always preserved.
        Recurses into all nested containers.
        """
        ...

    @classmethod
    def from_list(
        cls,
        obj: "list[Any]",
        *,
        schema: "Schema | None" = None,
    ) -> "YamlSequence":
        """Create a ``YamlSequence`` from a plain Python list (or any iterable).

        Nested dicts are recursively converted to ``YamlMapping``, nested lists to
        ``YamlSequence``. Raises ``TypeError`` if *obj* cannot be interpreted as a sequence.
        """
        ...

    def __copy__(self) -> "YamlSequence": ...
    def __deepcopy__(self, memo: "dict[int, Any]") -> "YamlSequence": ...

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

        To control the emitted YAML style, pass a ``YamlScalar``, ``YamlMapping``,
        or ``YamlSequence`` as *data* — style and tag metadata on the node are
        preserved, and the *tag* from the tuple is applied on top::

            schema.add_dumper(MyType, lambda obj: ("!mytag", yarutsk.YamlScalar(str(obj), style="double")))
            schema.add_dumper(Point, lambda p: ("!point", yarutsk.YamlMapping(style="flow")))
        """
        ...

# ── Module-level functions ────────────────────────────────────────────────────

class YamlIter:
    """Lazy document iterator returned by :func:`iter_load_all` and
    :func:`iter_loads_all`.

    Yields one document at a time via ``__next__``, never accumulating the
    entire multi-document stream in memory.

    Example::

        for doc in yarutsk.iter_load_all(open("huge.yaml")):
            process(doc)
    """

    def __iter__(self) -> "YamlIter": ...
    def __next__(self) -> "YamlMapping | YamlSequence | YamlScalar": ...

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

def iter_load_all(
    stream: IO[str] | IO[bytes],
    *,
    schema: Schema | None = None,
) -> YamlIter:
    """Return a lazy iterator that yields YAML documents from *stream* one at a
    time without reading the entire file into memory first."""
    ...

def iter_loads_all(
    text: str,
    *,
    schema: Schema | None = None,
) -> YamlIter:
    """Return a lazy iterator that yields YAML documents from *text* one at a
    time."""
    ...

def dump(
    doc: "_Dumpable",
    stream: IO[str] | IO[bytes],
    *,
    schema: Schema | None = None,
    indent: int = 2,
) -> None:
    """Serialize *doc* to *stream* in block-style YAML.

    *doc* can be a ``YamlMapping``/``YamlSequence``/``YamlScalar``, or a plain
    Python ``dict``/``list``/``tuple``/scalar which will be auto-converted.

    *indent* controls the per-level indentation width (default: 2).
    """
    ...

def dumps(
    doc: "_Dumpable",
    *,
    schema: Schema | None = None,
    indent: int = 2,
) -> str:
    """Serialize *doc* to a YAML string.

    *doc* can be a ``YamlMapping``/``YamlSequence``/``YamlScalar``, or a plain
    Python ``dict``/``list``/``tuple``/scalar which will be auto-converted.

    *indent* controls the per-level indentation width (default: 2).
    """
    ...

def dump_all(
    docs: "_Sequence[_Dumpable]",
    stream: IO[str] | IO[bytes],
    *,
    schema: Schema | None = None,
    indent: int = 2,
) -> None:
    """Serialize multiple documents to *stream*, separated by ``---``.

    *indent* controls the per-level indentation width (default: 2).
    """
    ...

def dumps_all(
    docs: "_Sequence[_Dumpable]",
    *,
    schema: Schema | None = None,
    indent: int = 2,
) -> str:
    """Serialize multiple documents to a string, separated by ``---``.

    *indent* controls the per-level indentation width (default: 2).
    """
    ...
