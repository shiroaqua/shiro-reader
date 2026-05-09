use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct DataResponse<T>
where
    T: Serialize,
{
    pub data: T,
}

impl<T> DataResponse<T>
where
    T: Serialize,
{
    pub fn new(data: T) -> Self {
        Self { data }
    }
}
