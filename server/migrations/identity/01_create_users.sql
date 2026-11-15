PRAGMA foreign_keys = ON;

-- #[toasty::breakpoint]
CREATE TABLE IF NOT EXISTS users (
    id BLOB NOT NULL PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
