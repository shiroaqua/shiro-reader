PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS folders (
    id            TEXT    NOT NULL PRIMARY KEY,
    bookshelf_id  TEXT    NOT NULL,
    parent_id     TEXT,
    name          TEXT    NOT NULL,
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL,


    CONSTRAINT chk_folders_id_uuid
        CHECK (
            length(id) = 36
            AND substr(id, 9, 1) = '-'
            AND substr(id, 14, 1) = '-'
            AND substr(id, 19, 1) = '-'
            AND substr(id, 24, 1) = '-'
            AND lower(id) = id
            AND replace(id, '-', '') NOT GLOB '*[^0-9a-f]*'
        ),

    CONSTRAINT chk_bookshelf_id
        CHECK (
            length(bookshelf_id) = 36
            AND substr(bookshelf_id, 9, 1) = '-'
            AND substr(bookshelf_id, 14, 1) = '-'
            AND substr(bookshelf_id, 19, 1) = '-'
            AND substr(bookshelf_id, 24, 1) = '-'
            AND lower(bookshelf_id) = bookshelf_id
            AND replace(bookshelf_id, '-', '') NOT GLOB '*[^0-9a-f]*'
        ),


    CONSTRAINT chk_folders_parent_id_uuid
        CHECK (
            length(parent_id) = 36
            AND substr(parent_id, 9, 1) = '-'
            AND substr(parent_id, 14, 1) = '-'
            AND substr(parent_id, 19, 1) = '-'
            AND substr(parent_id, 24, 1) = '-'
            AND lower(parent_id) = parent_id
            AND replace(parent_id, '-', '') NOT GLOB '*[^0-9a-f]*'           
        ),

    CONSTRAINT chk_folders_name_valid
        CHECK (length(name) > 0),

    CONSTRAINT chk_folders_created_at_positive
        CHECK (created_at > 0),

    CONSTRAINT chk_folders_updated_at_valid
        CHECK (
            updated_at > 0
            AND updated_at >= created_at
        ),

    -- 同一父集下不允许两个同名文件夹。
    CONSTRAINT uq_folders_parent_name
        UNIQUE (bookshelf_id, parent_id, name),

    CONSTRAINT fk_folders_parent
        FOREIGN KEY (parent_id)
        REFERENCES bookshelves(id)
        ON UPDATE CASCADE
        ON DELETE CASCADE
);


-- 禁止修改文件夹 id 和 created_at。
CREATE TRIGGER IF NOT EXISTS trg_folders_immutable_columns
BEFORE UPDATE OF id, created_at ON folders
FOR EACH ROW
BEGIN
    SELECT RAISE(
        ABORT,
        'folders.id and folders.created_at are immutable'
    );
END;


-- 禁止 updated_at 倒退。
CREATE TRIGGER IF NOT EXISTS trg_folders_updated_at_monotonic
BEFORE UPDATE OF updated_at ON folders
FOR EACH ROW
WHEN NEW.updated_at < OLD.updated_at
BEGIN
    SELECT RAISE(
        ABORT,
        'folders.updated_at cannot move backwards'
    );
END;


-- 防止文件夹移动后形成环。
CREATE TRIGGER IF NOT EXISTS trg_folders_no_cycle_on_move
BEFORE UPDATE OF parent_id ON folders
FOR EACH ROW
WHEN NEW.parent_id IS NOT OLD.parent_id
BEGIN
    SELECT RAISE(
        ABORT,
        'folder cycle detected'
    )
    WHERE EXISTS (
        WITH RECURSIVE ancestors(id, parent_id) AS (
            SELECT
                d.id,
                d.parent_id
            FROM folders AS d
            WHERE d.id = NEW.parent_id

            UNION ALL

            SELECT
                parent.id,
                parent.parent_id
            FROM folders AS parent
            INNER JOIN ancestors AS child
                ON parent.id = child.parent_id
            WHERE child.parent_id IS NOT NULL
        )
        SELECT 1
        FROM ancestors
        WHERE id = OLD.id
    );
END;