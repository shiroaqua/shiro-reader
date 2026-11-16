CREATE TABLE IF NOT EXISTS tag_definitions (
    id BLOB NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    parent_id BLOB REFERENCES tag_definitions(id) ON DELETE RESTRICT,
    color TEXT NOT NULL,
    value_rule TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- #[toasty::breakpoint]
CREATE INDEX IF NOT EXISTS idx_tag_definitions_parent_id ON tag_definitions(parent_id);

-- #[toasty::breakpoint]
CREATE TABLE IF NOT EXISTS book_tags (
    tag_id BLOB NOT NULL REFERENCES tag_definitions(id) ON DELETE CASCADE,
    book_id BLOB NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    value TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tag_id, book_id)
);

-- #[toasty::breakpoint]
CREATE INDEX IF NOT EXISTS idx_book_tags_book_id ON book_tags(book_id);
