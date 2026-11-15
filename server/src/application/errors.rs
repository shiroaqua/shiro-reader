pub trait MapToastyError<E> {
    fn map_db_err(error: toasty::Error) -> E;
}