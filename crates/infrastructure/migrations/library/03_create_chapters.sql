CREATE TABLE IF NOT EXISTS chapters (
    id BLOB NOT NULL PRIMARY KEY,
    book_id BLOB NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    title TEXT NOT NULL,
    location_json TEXT NOT NULL
);

-- #[toasty::breakpoint]
CREATE UNIQUE INDEX IF NOT EXISTS idx_chapters_book_position
ON chapters(book_id, position);
