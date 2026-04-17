# Limitations

- **Integer range**: integers are stored as 64-bit signed (`i64`). Values outside `[-9223372036854775808, 9223372036854775807]` are loaded as strings.
- **Underscore separators**: `1_000` is not parsed as an integer — it is loaded as the string `"1_000"` (and round-tripped faithfully as such).
- **Blank line cap**: at most 255 blank lines before any entry are tracked; runs longer than that are clamped to 255 on load.
- **Block only by default**: the emitter writes block-style YAML. Flow containers (`{...}` / `[...]`) from the source are preserved if they were already flow-style, but there is no option to force everything to flow on dump.
- **Memory per document**: each individual document must fit in memory. `load` / `load_all` stream from IO in 8 KB chunks so the raw source text is not buffered as a whole, but the resulting Python objects still live in memory. For large multi-document streams use `iter_load_all` / `iter_loads_all` to process one document at a time without accumulating the full list.
- **YAML version**: the scanner implements YAML 1.1 boolean/null coercion (`yes`/`no`/`on`/`off`/`~`). Most YAML 1.2-only documents load correctly, but inputs that rely on strict YAML 1.2 semantics may differ.
