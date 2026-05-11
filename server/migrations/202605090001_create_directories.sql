PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS directories (
    id         TEXT    NOT NULL,
    parent_id  TEXT,
    name       TEXT    NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,

    CONSTRAINT pk_directories
        PRIMARY KEY (id),

    CONSTRAINT chk_directories_id_uuid
        CHECK (
            length(id) = 36
            AND substr(id, 9, 1) = '-'
            AND substr(id, 14, 1) = '-'
            AND substr(id, 19, 1) = '-'
            AND substr(id, 24, 1) = '-'
            AND lower(id) = id
            AND replace(id, '-', '') NOT GLOB '*[^0-9a-f]*'
        ),

    CONSTRAINT chk_directories_parent_id_uuid
        CHECK (
            parent_id IS NULL
            OR (
                length(parent_id) = 36
                AND substr(parent_id, 9, 1) = '-'
                AND substr(parent_id, 14, 1) = '-'
                AND substr(parent_id, 19, 1) = '-'
                AND substr(parent_id, 24, 1) = '-'
                AND lower(parent_id) = parent_id
                AND replace(parent_id, '-', '') NOT GLOB '*[^0-9a-f]*'
            )
        ),

    -- 根目录是唯一允许 parent_id IS NULL 的目录。
    -- 普通目录必须有父目录。
    CONSTRAINT chk_directories_root_shape
        CHECK (
            (
                id = '00000000-0000-0000-0000-000000000000'
                AND parent_id IS NULL
                AND name = '$ROOT'
            )
            OR
            (
                id <> '00000000-0000-0000-0000-000000000000'
                AND parent_id IS NOT NULL
                AND parent_id <> id
                AND name <> '$ROOT'
            )
        ),

    -- 普通目录名：
    -- 1. 非空
    -- 2. 禁止首尾空白
    -- 3. 禁止符号$
    CONSTRAINT chk_directories_name_valid
        CHECK (
            (
                id = '00000000-0000-0000-0000-000000000000'
                AND name = '$ROOT'
            )
            OR
            (
                length(name) > 0
                AND name = trim(name)
                AND name <> '$ROOT'
                AND instr(name, '$') = 0
            )
        ),

    CONSTRAINT chk_directories_created_at_positive
        CHECK (created_at > 0),

    CONSTRAINT chk_directories_updated_at_valid
        CHECK (
            updated_at > 0
            AND updated_at >= created_at
        ),

    -- 同一父目录下不允许两个同名目录。
    -- 因为普通目录 parent_id 必须非空，所以可以直接使用普通 UNIQUE。
    CONSTRAINT uq_directories_parent_name
        UNIQUE (parent_id, name),

    CONSTRAINT fk_directories_parent
        FOREIGN KEY (parent_id)
        REFERENCES directories(id)
        ON UPDATE CASCADE
        ON DELETE CASCADE
);


-- 初始化根目录。
-- now_ms 使用 SQLite 内置 julianday 计算 Unix 毫秒时间戳。
INSERT OR IGNORE INTO directories (
    id,
    parent_id,
    name,
    created_at,
    updated_at
)
VALUES (
    '00000000-0000-0000-0000-000000000000',
    NULL,
    '$ROOT',
    CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER),
    CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER)
);


-- 禁止删除根目录。
CREATE TRIGGER IF NOT EXISTS trg_directories_no_delete_root
BEFORE DELETE ON directories
FOR EACH ROW
WHEN OLD.id = '00000000-0000-0000-0000-000000000000'
BEGIN
    SELECT RAISE(
        ABORT,
        'root directory cannot be deleted'
    );
END;


-- 禁止修改目录 id 和 created_at。
CREATE TRIGGER IF NOT EXISTS trg_directories_immutable_columns
BEFORE UPDATE OF id, created_at ON directories
FOR EACH ROW
BEGIN
    SELECT RAISE(
        ABORT,
        'directories.id and directories.created_at are immutable'
    );
END;


-- 禁止修改根目录结构字段。
-- 根目录允许 updated_at 改变，因为根目录内容可能变化。
CREATE TRIGGER IF NOT EXISTS trg_directories_root_structure_immutable
BEFORE UPDATE OF parent_id, name ON directories
FOR EACH ROW
WHEN OLD.id = '00000000-0000-0000-0000-000000000000'
BEGIN
    SELECT RAISE(
        ABORT,
        'root directory structure is immutable'
    );
END;


-- 禁止 updated_at 倒退。
CREATE TRIGGER IF NOT EXISTS trg_directories_updated_at_monotonic
BEFORE UPDATE OF updated_at ON directories
FOR EACH ROW
WHEN NEW.updated_at < OLD.updated_at
BEGIN
    SELECT RAISE(
        ABORT,
        'directories.updated_at cannot move backwards'
    );
END;



-- 防止目录移动后形成环。
CREATE TRIGGER IF NOT EXISTS trg_directories_no_cycle_on_move
BEFORE UPDATE OF parent_id ON directories
FOR EACH ROW
WHEN NEW.parent_id IS NOT OLD.parent_id
BEGIN
    SELECT RAISE(
        ABORT,
        'directory cycle detected'
    )
    WHERE EXISTS (
        WITH RECURSIVE ancestors(id, parent_id) AS (
            SELECT
                d.id,
                d.parent_id
            FROM directories AS d
            WHERE d.id = NEW.parent_id

            UNION ALL

            SELECT
                parent.id,
                parent.parent_id
            FROM directories AS parent
            INNER JOIN ancestors AS child
                ON parent.id = child.parent_id
            WHERE child.parent_id IS NOT NULL
        )
        SELECT 1
        FROM ancestors
        WHERE id = OLD.id
    );
END;