PRAGMA foreign_keys = ON;

-- #[toasty::breakpoint]
PRAGMA synchronous = NORMAL;

-- #[toasty::breakpoint]
CREATE TABLE IF NOT EXISTS books (
    id BLOB NOT NULL PRIMARY KEY,
    title TEXT NOT NULL,
    hash BLOB NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT
);

-- #[toasty::breakpoint]
CREATE UNIQUE INDEX IF NOT EXISTS idx_books_active_title
ON books(title)
WHERE deleted_at IS NULL;
