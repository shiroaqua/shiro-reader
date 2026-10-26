use crate::{
    api::v1::auth::dto::{LoginResponse, RefreshAccessTokenResponse},
    application::identity::authentication::dto::{LoginOutput, RefreshAccessTokenOutput},
};

impl From<LoginOutput> for LoginResponse {
    fn from(output: LoginOutput) -> Self {
        Self {
            access_token: output.access_token,
            refresh_token: output.refresh_token,
        }
    }
}

impl From<RefreshAccessTokenOutput> for RefreshAccessTokenResponse {
    fn from(output: RefreshAccessTokenOutput) -> Self {
        Self {
            access_token: output.access_token,
        }
    }
}
