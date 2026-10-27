//! Shared font metadata, without permanently retaining font file contents.
//! USVG reads/maps the faces actually used by each tree; dropping that tree
//! releases those temporary resources after rasterization.
use std::sync::{Arc, LazyLock};
use usvg::fontdb::Database;

pub(crate) fn system_fonts() -> Arc<Database> {
    static FONTS: LazyLock<Arc<Database>> = LazyLock::new(|| {
        let mut db = Database::new();
        db.load_system_fonts();
        Arc::new(db)
    });
    FONTS.clone()
}
