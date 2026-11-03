-- Only cryptographic digests are stored; neither table contains bearer credentials.
CREATE TABLE auth_sessions (
    refresh_hash TEXT NOT NULL PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE
);
CREATE INDEX idx_auth_sessions_user ON auth_sessions(user_id);

CREATE TABLE auth_access_tokens (
    access_hash TEXT NOT NULL PRIMARY KEY,
    refresh_hash TEXT NOT NULL REFERENCES auth_sessions(refresh_hash) ON DELETE CASCADE
);
CREATE INDEX idx_auth_access_session ON auth_access_tokens(refresh_hash);
