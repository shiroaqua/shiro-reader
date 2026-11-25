#[derive(Debug)]
pub struct GetBooksQuery {
    pub ids: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct GetBookNavigationQuery {
    pub id: String,
}
