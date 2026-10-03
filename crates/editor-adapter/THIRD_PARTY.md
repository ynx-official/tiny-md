# Guise editor adaptation

`src/markdown_editor.rs`, `src/editmenu.rs`, and `src/chord.rs` originate from
the published `guise-ui` **1.9.1** crate, https://github.com/wess/guise,
under the MIT license retained in `LICENSE.guise`.

Only these view and private-helper files are copied. The document model,
Markdown block/inline parsing, layout plans, source mapping, highlighting,
icons, menus, and theme are still imported from the pinned Guise dependency.

Local changes add GPUI's platform text-input contract (UTF-16 ranges,
composition, candidate geometry), preserve one history step for a composition,
and support source mode without replacing the editor or its history.

When upgrading Guise, compare these files against the published sources and
run the input regressions before bumping the exact dependency version.
