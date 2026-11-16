pub mod identity;
pub mod library;

pub trait EntityId {
    type Id;

    fn id(&self) -> &Self::Id;
}

pub trait EntityCreate: Sized {
    type Create;

    fn create(self) -> Self::Create;
}

pub trait RequiresVerificationEntity<E> {
    fn verify(self) -> Result<Self, E>
    where
        Self: Sized;
}
