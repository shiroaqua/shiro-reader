PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS users (
    id              TEXT    NOT NULL PRIMARY KEY,
    name            TEXT    NOT NULL UNIQUE,
    email           TEXT,
    phone           TEXT,
    phone_extension TEXT,
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

    CONSTRAINT chk_users_email_non_empty
        CHECK (email IS NULL OR (length(email) > 0 AND email = trim(email))),

    -- 主号码保存 E.164 格式，分机单独保存以保留前导零。
    CONSTRAINT chk_users_phone_e164
        CHECK (
            phone IS NULL
            OR (
                length(phone) BETWEEN 2 AND 16
                AND substr(phone, 1, 1) = '+'
                AND substr(phone, 2, 1) BETWEEN '1' AND '9'
                AND substr(phone, 2) NOT GLOB '*[^0-9]*'
            )
        ),

    CONSTRAINT chk_users_phone_extension
        CHECK (
            phone_extension IS NULL
            OR (
                phone IS NOT NULL
                AND length(phone_extension) > 0
                AND phone_extension NOT GLOB '*[^0-9]*'
            )
        ),

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
