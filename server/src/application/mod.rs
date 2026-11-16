pub mod errors;
pub mod identity;
pub mod library;

pub trait MapDbResult<T, E> {
    fn map_result(self) -> Result<T, E>;
}
