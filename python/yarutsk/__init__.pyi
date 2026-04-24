"""A YAML round-trip library that preserves comments and insertion order."""

import datetime as _datetime
from collections.abc import Callable, Iterable
from collections.abc import Mapping as _Mapping
from collections.abc import Sequence as _Sequence
from typing import (
    IO,
    Any,
    Literal,
    SupportsIndex,
    TypeVar,
)

_T = TypeVar("_T")

# ── Public type aliases ───────────────────────────────────────────────────────

type ScalarStyle = Literal["plain", "single", "double", "literal", "folded"]
"""The quoting style of a YAML scalar."""

type ContainerStyle = Literal["block", "flow"]
"""The layout style of a YAML mapping or sequence."""

type YamlNode = YamlMapping | YamlSequence | YamlScalar
"""Any YAML document node (mapping, sequence, or scalar)."""

# ── Internal aliases ──────────────────────────────────────────────────────────

# The Python value of a scalar leaf after tag handling.
# ``!!binary`` tags yield ``bytes`` and ``!!timestamp`` tags yield
# ``datetime``/``date``; all other scalars are primitives.
_Scalar = int | float | bool | str | None | bytes | _datetime.datetime | _datetime.date

# Values accepted by the YamlScalar constructor.
_ScalarInit = _Scalar | bytearray

# Any value accepted by dump/dumps — any YAML node, abstract containers,
# bytes, or scalar primitive.
type _Dumpable = YamlNode | _Mapping[str, Any] | Iterable[Any] | bytearray | _Scalar

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
    """A YAML scalar document node.

    Can be constructed directly to create a styled scalar for assignment or
    use in a Schema dumper::

        doc["key"] = yarutsk.YamlScalar("hello", style="double")
        doc["data"] = yarutsk.YamlScalar(b"binary data")          # !!binary
        doc["when"] = yarutsk.YamlScalar(datetime.date(2024, 1, 1))  # !!timestamp
        schema.add_dumper(MyType, lambda obj: ("!mytag", yarutsk.YamlScalar(str(obj), style="single")))
    """

    def __init__(
        self,
        value: _ScalarInit,
        *,
        style: ScalarStyle = "plain",
        tag: str | None = None,
    ) -> None:
        """Create a scalar with the given value, quoting style, and optional tag.

        *value* can be ``str``, ``int``, ``float``, ``bool``, ``None``,
        ``bytes``, ``bytearray``, ``datetime.datetime``, or ``datetime.date``.

        ``bytes``/``bytearray`` values are base64-encoded and tagged ``!!binary``
        by default. ``datetime``/``date`` values are ISO-formatted and tagged
        ``!!timestamp`` by default. Pass *tag* to override the default tag.

        *style* must be one of ``"plain"``, ``"single"``, ``"double"``, ``"literal"``, ``"folded"``.
        Raises ``TypeError`` if *value* is not an accepted type.
        Raises ``ValueError`` for an unknown *style*.
        """
        ...

    @property
    def value(self) -> _Scalar:
        """The Python value of this scalar.

        Applies built-in tag handling: ``!!binary`` → ``bytes``,
        ``!!timestamp`` → ``datetime.datetime`` / ``datetime.date``. All other
        tags yield the raw primitive (``int | float | bool | str | None``).
        """
        ...

    @property
    def style(self) -> ScalarStyle:
        """The scalar quoting style: ``"plain"``, ``"single"``, ``"double"``, ``"literal"``, or ``"folded"``."""
        ...

    @style.setter
    def style(self, value: ScalarStyle) -> None: ...
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
    @property
    def blank_lines_before(self) -> int:
        """Number of blank lines emitted before this scalar (0-255)."""
        ...

    @blank_lines_before.setter
    def blank_lines_before(self, value: int) -> None: ...
    def format(self, *, styles: bool = True, comments: bool = True) -> None:
        """Strip cosmetic scalar formatting, resetting to clean YAML defaults.

        When *styles* is ``True`` (the default), scalar quoting → plain (literal
        for multi-line strings) and ``original`` is cleared so non-canonical
        forms emit canonically. When *comments* is ``True`` (the default), any
        ``comment_inline`` or ``comment_before`` attached to this scalar is
        cleared. Tags and anchors are always preserved.
        """
        ...

    @property
    def comment_inline(self) -> str | None:
        """The inline comment on this scalar (text after ``#``, no leading ``#``), or ``None``. Assign ``None`` to clear."""
        ...

    @comment_inline.setter
    def comment_inline(self, value: str | None) -> None: ...
    @property
    def comment_before(self) -> str | None:
        """The block comment preceding this scalar, or ``None``. Assign ``None`` to clear."""
        ...

    @comment_before.setter
    def comment_before(self, value: str | None) -> None: ...
    def to_python(self) -> _Scalar:
        """Return the Python primitive value."""
        ...

    def __eq__(self, other: object) -> bool: ...
    def __repr__(self) -> str: ...

class YamlMapping(dict[str, Any]):
    """A YAML mapping node. Subclass of dict — all standard dict operations work.

    In addition to the full dict interface, provides:
    - Comment access/mutation methods
    - ``sort_keys()`` for in-place key sorting
    - ``to_python()`` for deep conversion to a plain Python ``dict``

    Can be constructed directly to create a styled mapping::

        m = yarutsk.YamlMapping(style="flow")
        m["x"] = 1
        m["y"] = 2
        doc["point"] = m
    """

    def __init__(
        self,
        mapping: _Mapping[str, Any] | YamlMapping | Iterable[tuple[str, Any]] | None = None,
        *,
        style: Literal["block", "flow"] = "block",
        tag: str | None = None,
        schema: Schema | None = None,
    ) -> None:
        """Create a mapping, optionally populated from *mapping*.

        If *mapping* is a ``YamlMapping``, inner metadata (comments, styles,
        anchors) is preserved. Any other mapping (anything with a ``keys()``
        method) or iterable of ``(key, value)`` pairs is accepted and entries
        are set as plain values.

        If *schema* is provided, schema dumpers are applied during conversion
        of nested values. Raises ``ValueError`` for an unknown *style*.
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
        """Number of blank lines emitted after the last entry in this mapping (0-255)."""
        ...

    @trailing_blank_lines.setter
    def trailing_blank_lines(self, value: int) -> None: ...
    @property
    def blank_lines_before(self) -> int:
        """Number of blank lines emitted before this mapping (0-255)."""
        ...

    @blank_lines_before.setter
    def blank_lines_before(self, value: int) -> None: ...
    def node(self, key: str) -> YamlNode:
        """Return the underlying YAML node for *key*, preserving style/tag metadata.
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
        """Sort mapping keys in-place.

        When *recursive* is ``True``, every nested ``YamlMapping`` (including
        ones reached through nested ``YamlSequence`` items) has its keys
        sorted with the same *key* / *reverse* arguments. Sequence item order
        is **not** changed — ``sort_keys`` only touches mapping keys.
        """
        ...

    def to_python(self) -> Any:
        """Recursively convert to a plain Python ``dict`` (no YamlMapping/YamlSequence nodes)."""
        ...

    @property
    def comment_inline(self) -> str | None:
        """The inline comment on this mapping (text after ``#``, no leading ``#``), or ``None``. Assign ``None`` to clear."""
        ...

    @comment_inline.setter
    def comment_inline(self, value: str | None) -> None: ...
    @property
    def comment_before(self) -> str | None:
        """The block comment preceding this mapping, lines joined with ``\\n``, or ``None`` if unset. Assign ``None`` to clear."""
        ...

    @comment_before.setter
    def comment_before(self, value: str | None) -> None: ...
    def get_alias(self, key: str) -> str | None:
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

    def nodes(self) -> list[tuple[str, YamlNode]]:
        """Return a list of ``(key, node)`` pairs for all entries in this mapping.

        Each node is a ``YamlMapping``, ``YamlSequence``, or ``YamlScalar``,
        preserving style/tag metadata. Unlike ``items()``, which returns plain
        Python values, ``nodes()`` returns the full typed node objects.
        """
        ...

    def copy(self) -> YamlMapping: ...
    def __copy__(self) -> YamlMapping: ...
    def __deepcopy__(self, memo: dict[int, Any]) -> YamlMapping: ...

class YamlSequence(list[Any]):
    """A YAML sequence node. Subclass of list — all standard list operations work.

    In addition to the full list interface, provides:
    - Comment access/mutation methods (addressed by integer index)
    - ``sort()`` override that preserves comment metadata
    - ``to_python()`` for deep conversion to a plain Python ``list``

    Can be constructed directly to create a styled sequence::

        s = yarutsk.YamlSequence(style="flow")
        s.extend([1, 2, 3])
        doc["items"] = s
    """

    def __init__(
        self,
        iterable: Iterable[Any] | None = None,
        *,
        style: Literal["block", "flow"] = "block",
        tag: str | None = None,
        schema: Schema | None = None,
    ) -> None:
        """Create a sequence, optionally populated from *iterable*.

        If *iterable* is a ``YamlSequence``, inner metadata (comments, styles,
        anchors) is preserved. Any other iterable has its items appended as plain
        values.

        If *schema* is provided, schema dumpers are applied during conversion
        of nested values. Raises ``ValueError`` for an unknown *style*.
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
        """Number of blank lines emitted after the last item in this sequence (0-255)."""
        ...

    @trailing_blank_lines.setter
    def trailing_blank_lines(self, value: int) -> None: ...
    def node(self, idx: int) -> YamlNode:
        """Return the underlying YAML node for the item at *idx*, preserving style/tag metadata.
        Raises ``IndexError`` for out-of-range indices.
        """
        ...

    def nodes(self) -> list[YamlNode]:
        """Return the underlying YAML node for every item, in order.

        Each node is a ``YamlMapping``, ``YamlSequence``, or ``YamlScalar``,
        preserving style/tag metadata. Unlike iterating the sequence (which
        yields Python primitives for scalars), ``nodes()`` returns full typed
        node objects.
        """
        ...

    @property
    def blank_lines_before(self) -> int:
        """Number of blank lines emitted before this sequence (0-255)."""
        ...

    @blank_lines_before.setter
    def blank_lines_before(self, value: int) -> None: ...
    def clear(self) -> None:
        """Remove all items from this sequence."""
        ...

    def index(self, value: object, start: SupportsIndex = ..., stop: SupportsIndex = ...) -> int:
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

    def to_python(self) -> Any:
        """Recursively convert to a plain Python ``list`` (no YamlMapping/YamlSequence nodes)."""
        ...

    @property
    def comment_inline(self) -> str | None:
        """The inline comment on this sequence (text after ``#``, no leading ``#``), or ``None``. Assign ``None`` to clear."""
        ...

    @comment_inline.setter
    def comment_inline(self, value: str | None) -> None: ...
    @property
    def comment_before(self) -> str | None:
        """The block comment preceding this sequence, lines joined with ``\\n``, or ``None`` if unset. Assign ``None`` to clear."""
        ...

    @comment_before.setter
    def comment_before(self, value: str | None) -> None: ...
    def get_alias(self, idx: int) -> str | None:
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

    def __copy__(self) -> YamlSequence: ...
    def __deepcopy__(self, memo: dict[int, Any]) -> YamlSequence: ...

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

        For a tagged scalar node, the callable receives the default-converted
        Python value (``str``, ``int``, ``float``, ``bool``, or ``None``). For
        a tagged mapping or sequence node, it receives a ``YamlMapping`` or
        ``YamlSequence`` respectively.

        **Coercion tags are special.** If *tag* is a built-in YAML coercion
        tag — ``!!int``, ``!!bool``, ``!!null``, ``!!float``, or ``!!str`` —
        your loader receives the **raw YAML source string** instead of the
        coerced value. Registering a loader on such a tag disables the
        library's built-in coercion for that tag across the whole document
        so you can parse it yourself (e.g. to accept ``1_000`` as an integer,
        or to reject values the default coercion would accept).

        The return value replaces the node in the loaded document.
        """
        ...

    def add_dumper(self, py_type: type[_T], func: Callable[[_T], tuple[str, Any]]) -> None:
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

    def __iter__(self) -> YamlIter: ...
    def __next__(self) -> YamlNode: ...

def load(
    stream: IO[str] | IO[bytes],
    *,
    schema: Schema | None = None,
) -> YamlNode | None:
    """Parse the first YAML document from a stream. Returns ``None`` for empty input."""
    ...

def loads(
    text: str | bytes | bytearray,
    *,
    schema: Schema | None = None,
) -> YamlNode | None:
    """Parse the first YAML document from a string or UTF-8 bytes. Returns ``None`` for empty input.

    Raises ``UnicodeDecodeError`` if *text* is bytes/bytearray and not valid UTF-8.
    """
    ...

def load_all(
    stream: IO[str] | IO[bytes],
    *,
    schema: Schema | None = None,
) -> list[YamlNode]:
    """Parse all YAML documents from a stream, returning a list."""
    ...

def loads_all(
    text: str | bytes | bytearray,
    *,
    schema: Schema | None = None,
) -> list[YamlNode]:
    """Parse all YAML documents from a string or UTF-8 bytes, returning a list.

    Raises ``UnicodeDecodeError`` if *text* is bytes/bytearray and not valid UTF-8.
    """
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
    text: str | bytes | bytearray,
    *,
    schema: Schema | None = None,
) -> YamlIter:
    """Return a lazy iterator that yields YAML documents from *text* (a string or
    UTF-8 bytes) one at a time.

    Raises ``UnicodeDecodeError`` if *text* is bytes/bytearray and not valid UTF-8.
    """
    ...

def dump(
    doc: _Dumpable,
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
    doc: _Dumpable,
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
    docs: _Sequence[_Dumpable],
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
    docs: _Sequence[_Dumpable],
    *,
    schema: Schema | None = None,
    indent: int = 2,
) -> str:
    """Serialize multiple documents to a string, separated by ``---``.

    *indent* controls the per-level indentation width (default: 2).
    """
    ...
