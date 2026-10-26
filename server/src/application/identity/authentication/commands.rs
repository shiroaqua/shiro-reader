pub struct LoginCommand {
    pub username: String,
    pub password: String
}

pub struct RefreshAccessTokenCommand {
    pub token: String,
}

pub struct VerifyAccessToeknCommand {
    pub token: String
}
