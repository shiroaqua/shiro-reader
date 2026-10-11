PRAGMA foreign_keys = ON;


CREATE TABLE IF NOT EXISTS books (
    id           TEXT    NOT NULL PRIMARY KEY,
    title        TEXT    NOT NULL UNIQUE,  
    hash         TEXT    NOT NULL,
    created_at_high INTEGER NOT NULL,
    created_at_low INTEGER NOT NULL,
    updated_at_high INTEGER NOT NULL,
    updated_at_low INTEGER NOT NULL,
    version      INTEGER NOT NULL DEFAULT 0,

    CONSTRAINT chk_books_id_uuid
        CHECK (
            length(id) = 36
            AND substr(id, 9, 1) = '-'
            AND substr(id, 14, 1) = '-'
            AND substr(id, 19, 1) = '-'
            AND substr(id, 24, 1) = '-'
            AND lower(id) = id
            AND replace(id, '-', '') NOT GLOB '*[^0-9a-f]*'
        ),

    -- BLAKE3 默认 digest 的十六进制文本：32 bytes = 64 hex chars
    CONSTRAINT chk_books_hash_blake3_hex
        CHECK (
            length(hash) = 64
            AND lower(hash) = hash
            AND hash NOT GLOB '*[^0-9a-f]*'
        ),

    -- 书名不允许为空，也不允许首尾空白制造“看似同名但实际不同”的条目
    CONSTRAINT chk_books_title_non_empty
        CHECK (
            length(title) > 0
            AND title = trim(title)
        ),


    CONSTRAINT chk_books_created_at_positive
        CHECK (
            created_at_high > 0
            OR (created_at_high = 0 AND created_at_low > -9223372036854775807 - 1)
        ),

    CONSTRAINT chk_books_updated_at_valid
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


-- 禁止修改书籍条目的身份、物理文件 hash、创建时间。
CREATE TRIGGER IF NOT EXISTS trg_books_immutable_columns
BEFORE UPDATE OF id, hash, created_at_high, created_at_low ON books
FOR EACH ROW
BEGIN
    SELECT RAISE(
        ABORT,
        'books.id, books.hash and books.created_at are immutable'
    );
END;


-- updated_at 不允许倒退。
CREATE TRIGGER IF NOT EXISTS trg_books_updated_at_monotonic
BEFORE UPDATE OF updated_at_high, updated_at_low ON books
FOR EACH ROW
WHEN NEW.updated_at_high < OLD.updated_at_high
    OR (NEW.updated_at_high = OLD.updated_at_high AND NEW.updated_at_low < OLD.updated_at_low)
BEGIN
    SELECT RAISE(
        ABORT,
        'books.updated_at cannot move backwards'
    );
END;
