PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS bookshelves (
    id         TEXT    NOT NULL PRIMARY KEY,
    name       TEXT    NOT NULL UNIQUE,
    created_at_high INTEGER NOT NULL,
    created_at_low INTEGER NOT NULL,
    updated_at_high INTEGER NOT NULL,
    updated_at_low INTEGER NOT NULL,
    version    INTEGER NOT NULL DEFAULT 0,

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
        CHECK (
            created_at_high > 0
            OR (created_at_high = 0 AND created_at_low > -9223372036854775807 - 1)
        ),

    CONSTRAINT chk_bookshelves_updated_at_valid
        CHECK (
            (
                updated_at_high > 0
                OR (updated_at_high = 0 AND updated_at_low > -9223372036854775807 - 1)
            )
            AND (
                updated_at_high > created_at_high
                OR (updated_at_high = created_at_high AND updated_at_low >= created_at_low)
            )
        )
);


-- 禁止修改书架 id 和 created_at。
CREATE TRIGGER IF NOT EXISTS trg_bookshelves_immutable_columns
BEFORE UPDATE OF id, created_at_high, created_at_low ON bookshelves
FOR EACH ROW
BEGIN
    SELECT RAISE(
        ABORT,
        'bookshelves.id and bookshelves.created_at are immutable'
    );
END;


-- 禁止 updated_at 倒退。
CREATE TRIGGER IF NOT EXISTS trg_bookshelves_updated_at_monotonic
BEFORE UPDATE OF updated_at_high, updated_at_low ON bookshelves
FOR EACH ROW
WHEN NEW.updated_at_high < OLD.updated_at_high
    OR (NEW.updated_at_high = OLD.updated_at_high AND NEW.updated_at_low < OLD.updated_at_low)
BEGIN
    SELECT RAISE(
        ABORT,
        'bookshelves.updated_at cannot move backwards'
    );
END;
