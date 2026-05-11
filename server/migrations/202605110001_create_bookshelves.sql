PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS bookshelves (
    id         TEXT    NOT NULL PRIMARY KEY,
    name       TEXT    NOT NULL UNIQUE,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,

    CONSTRAINT chk_bookshelves_id_uuid
        CHECK (
            length(id) = 36
            AND substr(id, 9, 1) = '-'
            AND substr(id, 14, 1) = '-'
            AND substr(id, 19, 1) = '-'
            AND substr(id, 24, 1) = '-'
            AND lower(id) = id
            AND replace(id, '-', '') NOT GLOB '*[^0-9a-f]*'
        ),

    CONSTRAINT chk_bookshelves_name_valid
        CHECK (length(name) > 0),

    CONSTRAINT chk_bookshelves_created_at_positive
        CHECK (created_at > 0),

    CONSTRAINT chk_bookshelves_updated_at_valid
        CHECK (updated_at > 0 AND updated_at >= created_at)
);


-- 禁止修改书架 id 和 created_at。
CREATE TRIGGER IF NOT EXISTS trg_bookshelves_immutable_columns
BEFORE UPDATE OF id, created_at ON bookshelves
FOR EACH ROW
BEGIN
    SELECT RAISE(
        ABORT,
        'bookshelves.id and bookshelves.created_at are immutable'
    );
END;


-- 禁止 updated_at 倒退。
CREATE TRIGGER IF NOT EXISTS trg_bookshelves_updated_at_monotonic
BEFORE UPDATE OF updated_at ON bookshelves
FOR EACH ROW
WHEN NEW.updated_at < OLD.updated_at
BEGIN
    SELECT RAISE(
        ABORT,
        'bookshelves.updated_at cannot move backwards'
    );
END;

