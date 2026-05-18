use serde::{Deserialize};

#[derive(Debug, Deserialize)]
pub struct BookQuery {
    pub hash: String
}
