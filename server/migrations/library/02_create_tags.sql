CREATE TABLE tag_definitions (
    id           TEXT    NOT NULL PRIMARY KEY,
    name         TEXT    NOT NULL,
    parent_id    TEXT,
    allows_value BOOLEAN NOT NULL,
    value_regex  TEXT,
    created_at_high INTEGER NOT NULL,
    created_at_low INTEGER NOT NULL,
    updated_at_high INTEGER NOT NULL,
    updated_at_low INTEGER NOT NULL,
    version      BIGINT  NOT NULL DEFAULT 0,

    CHECK (version >= 0),
    CHECK (allows_value IN (0, 1)),
    CHECK (allows_value = 1 OR value_regex IS NULL),
    CHECK (parent_id IS NULL OR parent_id <> id),
    
    CONSTRAINT chk_tag_id_uuid
        CHECK (
            length(id) = 36
            AND substr(id, 9, 1) = '-'
            AND substr(id, 14, 1) = '-'
            AND substr(id, 19, 1) = '-'
            AND substr(id, 24, 1) = '-'
            AND lower(id) = id
            AND replace(id, '-', '') NOT GLOB '*[^0-9a-f]*'
        ),

    FOREIGN KEY (parent_id) REFERENCES tag_definitions (id) ON DELETE RESTRICT
);

CREATE INDEX idx_tag_definitions_parent_id ON tag_definitions (parent_id);

CREATE TABLE book_tags (
    book_id TEXT NOT NULL,
    tag_id  TEXT NOT NULL,
    value   TEXT,

    PRIMARY KEY (book_id, tag_id),
    FOREIGN KEY (book_id) REFERENCES books (id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id) REFERENCES tag_definitions (id) ON DELETE CASCADE
);

CREATE INDEX idx_book_tags_tag_id ON book_tags (tag_id);
