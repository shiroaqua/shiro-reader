use crate::{
    config::Config,
};

#[derive(Clone)]
pub struct AppState {

}

impl AppState {
    pub async fn build(config: &Config) -> anyhow::Result<Self> {
        Ok(Self {})
    }
}
