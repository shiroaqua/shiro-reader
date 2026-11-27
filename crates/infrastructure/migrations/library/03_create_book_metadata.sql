CREATE TABLE IF NOT EXISTS book_metadata (
    book_id BLOB NOT NULL PRIMARY KEY REFERENCES books(id) ON DELETE CASCADE,
    subtitle TEXT,
    authors TEXT NOT NULL,
    publisher TEXT,
    published_date TEXT,
    language TEXT,
    description TEXT,
    identifier TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT
);
