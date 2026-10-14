# Guise editor adaptation

`src/markdown_editor.rs`, `src/editmenu.rs`, and `src/chord.rs` originate from
the published `guise-ui` **1.9.1** crate, https://github.com/wess/guise,
under the MIT license retained in `LICENSE.guise`.

Only these view and private-helper files are copied. The document model,
Markdown block/inline parsing, layout plans, source mapping,
icons, menus, and theme are still imported from the pinned Guise dependency.

Local changes add GPUI's platform text-input contract (UTF-16 ranges,
composition, candidate geometry), preserve one history step for a composition,
and support source mode without replacing the editor or its history.
Fenced code uses Syntect 5.3.0 (MIT) grammars, mapped to Guise token colors;
Guise highlighting remains the fallback for source Markdown and missing grammars.

When upgrading Guise, compare these files against the published sources and
run the input regressions before bumping the exact dependency version.

# ELK layout dependency

Flowchart layout uses the pinned `elkrs` **0.1.1** dependency, a native Rust
ELK port published at https://crates.io/crates/elkrs, with API documentation at
https://docs.rs/elkrs/0.1.1/elkrs/ and upstream https://github.com/depetrol/elkrs.
Its published package declares Apache-2.0; the package's license is retained
in `LICENSE.elkrs`. Other Mermaid diagram types and fallback layouts continue
to use `mermaid-rs-renderer` 0.3.1 (MIT).

Release bundles retain the project, Guise and ELK licenses and this notice.
macOS stores them in `Contents/Resources/licenses/`; Windows carries their full
text in a source comment at the end of the bundled `welcome.md`, preserving the
three-file portable archive layout accepted by existing update clients.

The local adapter maps existing Mermaid node/text/style data into compound
ELK graphs and validates coordinates before using the existing SVG renderer.
When upgrading either dependency, run layout, shape, cache and concurrency
regressions and profile the complete reference document again.

`src/mermaid_parse.rs` keeps local compatibility handling for the pinned
renderer 0.3.1 preflight's capitalized END detection and pipe-labelled chains.
It uses the same arrow grammar and parses an equivalent temporary statement
copy; saved text and cache keys remain original. No registry source is patched.
On renderer upgrades, compare `validator::is_subgraph_close` and
`parser::split_edge_chain` against these regressions before removing the adapter.
