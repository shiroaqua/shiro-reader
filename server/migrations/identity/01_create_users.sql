PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS users (
    id              TEXT    NOT NULL PRIMARY KEY,
    name            TEXT    NOT NULL UNIQUE,
    password_hash   TEXT    NOT NULL,
    created_at_high INTEGER NOT NULL,
    created_at_low  INTEGER NOT NULL,
    updated_at_high INTEGER NOT NULL,
    updated_at_low  INTEGER NOT NULL,
    version         INTEGER NOT NULL DEFAULT 0,

    CONSTRAINT chk_users_id_uuid
        CHECK (
            length(id) = 36
            AND substr(id, 9, 1) = '-'
            AND substr(id, 14, 1) = '-'
            AND substr(id, 19, 1) = '-'
            AND substr(id, 24, 1) = '-'
            AND length(replace(id, '-', '')) = 32
            AND lower(id) = id
            AND replace(id, '-', '') NOT GLOB '*[^0-9a-f]*'
        ),

    CONSTRAINT chk_users_name_non_empty
        CHECK (length(name) > 0 AND name = trim(name)),

    CONSTRAINT chk_users_password_hash_non_empty
        CHECK (length(password_hash) > 0),

    CONSTRAINT chk_users_version_non_negative
        CHECK (typeof(version) = 'integer' AND version >= 0),

    CONSTRAINT chk_users_created_at_positive
        CHECK (
            created_at_high > 0
            OR (created_at_high = 0 AND created_at_low > -9223372036854775807 - 1)
        ),

    CONSTRAINT chk_users_updated_at_valid
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

-- 禁止修改用户身份和创建时间。
CREATE TRIGGER IF NOT EXISTS trg_users_immutable_columns
BEFORE UPDATE OF id, created_at_high, created_at_low ON users
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'users.id and users.created_at are immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_users_updated_at_monotonic
BEFORE UPDATE OF updated_at_high, updated_at_low ON users
FOR EACH ROW
WHEN NEW.updated_at_high < OLD.updated_at_high
    OR (NEW.updated_at_high = OLD.updated_at_high AND NEW.updated_at_low < OLD.updated_at_low)
BEGIN
    SELECT RAISE(ABORT, 'users.updated_at cannot move backwards');
END;


CREATE TRIGGER IF NOT EXISTS trg_users_version_monotonic
BEFORE UPDATE OF version ON users
FOR EACH ROW
WHEN NEW.version < OLD.version
BEGIN
    SELECT RAISE(ABORT, 'users.version cannot move backwards');
END;
