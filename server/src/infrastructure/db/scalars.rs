use toasty::{
    schema::{Field, Load},
    stmt::{self, Assign, Assignment, Expr, IntoExpr, List, Path, Value},
};

use crate::domain::library::{
    book::value_objects::BookHash,
    tag::value_objects::TagValueRule,
};

impl Load for BookHash {
    type Output = Self;

    fn ty() -> stmt::Type {
        stmt::Type::Bytes
    }

    fn load(value: Value) -> toasty::Result<Self> {
        let Value::Bytes(bytes) = value else {
            return Err(toasty::Error::invalid_result("book hash is not a blob"));
        };
        let bytes: [u8; 32] = bytes.try_into()
            .map_err(|_| toasty::Error::invalid_result("book hash must have 32 bytes"))?;
        Ok(Self(blake3::Hash::from_bytes(bytes)))
    }

    fn reload(target: &mut Self, value: Value) -> toasty::Result<()> {
        *target = Self::load(value)?;
        Ok(())
    }
}

impl IntoExpr<BookHash> for BookHash {
    fn into_expr(self) -> Expr<BookHash> {
        Expr::from_untyped(Value::Bytes(self.0.as_bytes().to_vec()))
    }

    fn by_ref(&self) -> Expr<BookHash> {
        self.clone().into_expr()
    }
}

impl Assign<BookHash> for BookHash {
    fn into_assignment(self) -> Assignment<BookHash> {
        stmt::set(self.into_expr())
    }
}

impl Field for BookHash {
    type ExprTarget = Self;
    type Path<Origin> = Path<Origin, Self>;
    type ListPath<Origin> = Path<Origin, List<Self>>;
    type Update<'a> = ();
    type Inner = Self;

    fn new_path<Origin>(path: Path<Origin, Self>) -> Self::Path<Origin> {
        path
    }

    fn new_list_path<Origin>(path: Path<Origin, List<Self>>) -> Self::ListPath<Origin> {
        path
    }

    fn new_update<'a>(_: &'a mut toasty_core::stmt::Assignments, _: stmt::Projection) -> Self::Update<'a> {}

    fn key_constraint<Origin>(&self, target: Path<Origin, Self::Inner>) -> Expr<bool> {
        target.eq(self.clone())
    }
}

impl Load for TagValueRule {
    type Output = Self;

    fn ty() -> stmt::Type {
        stmt::Type::String
    }

    fn load(value: Value) -> toasty::Result<Self> {
        let Value::String(encoded) = value else {
            return Err(toasty::Error::invalid_result("tag value rule is not text"));
        };
        let (allows, regex): (bool, Option<String>) = serde_json::from_str(&encoded)
            .map_err(|error| toasty::Error::invalid_result(error.to_string()))?;
        TagValueRule::new(allows, regex)
            .map_err(|error| toasty::Error::invalid_result(error.to_string()))
    }

    fn reload(target: &mut Self, value: Value) -> toasty::Result<()> {
        *target = Self::load(value)?;
        Ok(())
    }
}

impl IntoExpr<TagValueRule> for TagValueRule {
    fn into_expr(self) -> Expr<TagValueRule> {
        self.by_ref()
    }

    fn by_ref(&self) -> Expr<TagValueRule> {
        let encoded = serde_json::to_string(&(self.allows_value(), self.value_regex()))
            .expect("tag value rule is serializable");
        Expr::from_untyped(Value::String(encoded))
    }
}

impl Assign<TagValueRule> for TagValueRule {
    fn into_assignment(self) -> Assignment<TagValueRule> {
        stmt::set(self.into_expr())
    }
}

impl Field for TagValueRule {
    type ExprTarget = Self;
    type Path<Origin> = Path<Origin, Self>;
    type ListPath<Origin> = Path<Origin, List<Self>>;
    type Update<'a> = ();
    type Inner = Self;

    fn new_path<Origin>(path: Path<Origin, Self>) -> Self::Path<Origin> {
        path
    }

    fn new_list_path<Origin>(path: Path<Origin, List<Self>>) -> Self::ListPath<Origin> {
        path
    }

    fn new_update<'a>(_: &'a mut toasty_core::stmt::Assignments, _: stmt::Projection) -> Self::Update<'a> {}

    fn key_constraint<Origin>(&self, target: Path<Origin, Self::Inner>) -> Expr<bool> {
        target.eq(self.clone())
    }
}
