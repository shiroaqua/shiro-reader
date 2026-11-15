CREATE TABLE IF NOT EXISTS auth_sessions (
    refresh_hash TEXT NOT NULL PRIMARY KEY,
    user_id BLOB NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- #[toasty::breakpoint]
CREATE INDEX IF NOT EXISTS idx_auth_sessions_user ON auth_sessions(user_id);

-- #[toasty::breakpoint]
CREATE TABLE IF NOT EXISTS auth_access_tokens (
    access_hash TEXT NOT NULL PRIMARY KEY,
    refresh_hash TEXT NOT NULL REFERENCES auth_sessions(refresh_hash) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- #[toasty::breakpoint]
CREATE INDEX IF NOT EXISTS idx_auth_access_session ON auth_access_tokens(refresh_hash);
