PRAGMA foreign_keys = ON;


CREATE TABLE IF NOT EXISTS books (
    id           TEXT    NOT NULL PRIMARY KEY,
    title        TEXT    NOT NULL,
    hash         TEXT    NOT NULL,
    bookshelf_id TEXT    NOT NULL,
    folder_id    TEXT,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL,


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


    CONSTRAINT chk_books_bookshelf_id_uuid
        CHECK (
            length(bookshelf_id) = 36
            AND substr(bookshelf_id, 9, 1) = '-'
            AND substr(bookshelf_id, 14, 1) = '-'
            AND substr(bookshelf_id, 19, 1) = '-'
            AND substr(bookshelf_id, 24, 1) = '-'
            AND lower(bookshelf_id) = bookshelf_id
            AND replace(bookshelf_id, '-', '') NOT GLOB '*[^0-9a-f]*'
        ),

    CONSTRAINT chk_books_folder_id_uuid
        CHECK (
            length(folder_id) = 36
            AND substr(folder_id, 9, 1) = '-'
            AND substr(folder_id, 14, 1) = '-'
            AND substr(folder_id, 19, 1) = '-'
            AND substr(folder_id, 24, 1) = '-'
            AND lower(folder_id) = folder_id
            AND replace(folder_id, '-', '') NOT GLOB '*[^0-9a-f]*'
        ),

    CONSTRAINT chk_books_created_at_positive
        CHECK (created_at > 0),

    CONSTRAINT chk_books_updated_at_valid
        CHECK (
            updated_at > 0
            AND updated_at >= created_at
        ),

    CONSTRAINT fk_books_bookshelf
        FOREIGN KEY (bookshelf_id)
        REFERENCES bookshelves(id)
        ON UPDATE CASCADE
        ON DELETE CASCADE,

    CONSTRAINT fk_books_folder
        FOREIGN KEY (folder_id)
        REFERENCES folders(id)
        ON UPDATE CASCADE
        ON DELETE CASCADE
);


-- 禁止重复书名
CREATE UNIQUE INDEX uq_books_title_not_null 
ON books (bookshelf_id, folder_id, title) 
WHERE folder_id IS NOT NULL;

CREATE UNIQUE INDEX uq_books_title_null 
ON books (bookshelf_id, title) 
WHERE folder_id IS NULL;


-- 禁止修改书籍条目的身份、物理文件 hash、创建时间。
CREATE TRIGGER IF NOT EXISTS trg_books_immutable_columns
BEFORE UPDATE OF id, hash, created_at ON books
FOR EACH ROW
BEGIN
    SELECT RAISE(
        ABORT,
        'books.id, books.hash and books.created_at are immutable'
    );
END;


-- updated_at 不允许倒退。
CREATE TRIGGER IF NOT EXISTS trg_books_updated_at_monotonic
BEFORE UPDATE OF updated_at ON books
FOR EACH ROW
WHEN NEW.updated_at < OLD.updated_at
BEGIN
    SELECT RAISE(
        ABORT,
        'books.updated_at cannot move backwards'
    );
END;

