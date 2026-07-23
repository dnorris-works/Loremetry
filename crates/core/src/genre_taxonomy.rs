// genre_taxonomy.rs — thin wrapper over the database's genre table.
//
// The master genre list and the genre-to-KDP-path map now live in PostgreSQL
// (see db.rs) — this file just exposes them to the rest of the app and to
// the frontend. The JSON files in crates/core/data/ are used ONLY as one-time
// seed data by db.rs on first launch; after that, the database is
// authoritative and grows on its own as Category Finder discovers real paths.

use crate::db::{self, Db, GenreRow};

/// Everything needed to build the genre-ranking AI prompt: name + description
/// for every genre currently known to the database.
pub async fn master_genre_list(db: &Db) -> Result<Vec<GenreRow>, String> {
    db::list_genres(&db.pool).await
}

/// Known KDP path(s) for a genre name, in the given store.
pub async fn kdp_paths_for_genre(db: &Db, genre_name: &str, store: &str) -> Result<Vec<String>, String> {
    db::kdp_paths_for_genre(&db.pool, genre_name, store).await
}

/// Exposed to the frontend for reference/debugging — the live master list a
/// manuscript is scored against.
pub async fn get_genre_taxonomy(db: &Db) -> Result<Vec<GenreRow>, String> {
    master_genre_list(db).await
}
