use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoginOutput {
    pub user_id: String,
    pub username: String,
    pub access_token: String,
    pub refresh_token: String,
}

impl fmt::Debug for LoginOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LoginOutput")
            .field("user_id", &self.user_id)
            .field("username", &self.username)
            .field("access_token", &"[REDACTED]")
            .field("refresh_token", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefreshAccessTokenOutput {
    pub access_token: String,
}

impl fmt::Debug for RefreshAccessTokenOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RefreshAccessTokenOutput")
            .field("access_token", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyAccessTokenOutput {
    pub user_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_auth_outputs_redact_secrets_in_debug() {
        let login_resp = LoginOutput {
            user_id: "uid-1".to_owned(),
            username: "alice".to_owned(),
            access_token: "access-secret-123".to_owned(),
            refresh_token: "refresh-secret-456".to_owned(),
        };
        let resp_debug = format!("{login_resp:?}");
        assert!(resp_debug.contains("alice"));
        assert!(!resp_debug.contains("access-secret-123"));
        assert!(!resp_debug.contains("refresh-secret-456"));

        let refresh_resp = RefreshAccessTokenOutput {
            access_token: "new-access-secret".to_owned(),
        };
        assert!(!format!("{refresh_resp:?}").contains("new-access-secret"));
    }
}
