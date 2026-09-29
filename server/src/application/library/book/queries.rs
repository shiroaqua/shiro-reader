
#[derive(Debug)]
pub struct GetBooksQuery {
    pub bookshelf_id: String,
    pub book_ids: Vec<String>,
}