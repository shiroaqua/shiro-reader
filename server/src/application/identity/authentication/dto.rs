pub struct LoginOutput {
    pub access_token: String,
    pub refresh_token: String,
}

pub struct RefreshAccessTokenOutput {
    pub access_token: String,
}

pub struct VerifyAcessToeknOutput {
    pub user_id: String
}
