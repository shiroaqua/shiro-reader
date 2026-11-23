pub struct LoginCommand {
    pub username: String,
    pub password: String,
}

pub struct RefreshAccessTokenCommand {
    pub token: String,
}

pub struct VerifyAccessTokenCommand {
    pub token: String,
}

pub struct LogoutCommand {
    pub token: String,
}
