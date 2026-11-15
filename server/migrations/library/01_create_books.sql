PRAGMA foreign_keys = ON;

-- #[toasty::breakpoint]
CREATE TABLE IF NOT EXISTS books (
    id BLOB NOT NULL PRIMARY KEY,
    title TEXT NOT NULL UNIQUE,
    hash BLOB NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
