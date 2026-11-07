use serde::{Deserialize};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BookQuery {
    pub hash: String
}
