#[derive(Debug)]
pub struct GetBooksQuery {
    pub ids: Vec<String>,
}

#[derive(Debug)]
pub struct ListChaptersQuery {
    pub book_id: String,
}
