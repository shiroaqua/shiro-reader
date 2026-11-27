use jiff::Timestamp;
use shiro_domain::library::{
    book::value_objects::{BookAuthor, BookHash},
    tag::value_objects::TagValueRule,
};
use toasty::{
    schema::{Field, Load},
    stmt::{self, Assign, Assignment, Expr, IntoExpr, List, Path, Value},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DbBookHash(pub BookHash);

impl From<BookHash> for DbBookHash {
    fn from(value: BookHash) -> Self {
        Self(value)
    }
}

impl From<DbBookHash> for BookHash {
    fn from(value: DbBookHash) -> Self {
        value.0
    }
}

impl Load for DbBookHash {
    type Output = Self;

    fn ty() -> stmt::Type {
        stmt::Type::Bytes
    }

    fn load(value: Value) -> toasty::Result<Self> {
        let Value::Bytes(bytes) = value else {
            return Err(toasty::Error::invalid_result("book hash is not a blob"));
        };
        let bytes: [u8; 32] = bytes
            .try_into()
            .map_err(|_| toasty::Error::invalid_result("book hash must have 32 bytes"))?;
        Ok(Self(BookHash::from_bytes(bytes)))
    }

    fn reload(target: &mut Self, value: Value) -> toasty::Result<()> {
        *target = Self::load(value)?;
        Ok(())
    }
}

impl IntoExpr<DbBookHash> for DbBookHash {
    fn into_expr(self) -> Expr<DbBookHash> {
        Expr::from_untyped(Value::Bytes(self.0.as_bytes().to_vec()))
    }

    fn by_ref(&self) -> Expr<DbBookHash> {
        (*self).into_expr()
    }
}

impl Assign<DbBookHash> for DbBookHash {
    fn into_assignment(self) -> Assignment<DbBookHash> {
        stmt::set(self.into_expr())
    }
}

impl Field for DbBookHash {
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

    fn new_update<'a>(
        _: &'a mut toasty_core::stmt::Assignments,
        _: stmt::Projection,
    ) -> Self::Update<'a> {
    }

    fn key_constraint<Origin>(&self, target: Path<Origin, Self::Inner>) -> Expr<bool> {
        target.eq(*self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbTagValueRule(pub TagValueRule);

impl From<TagValueRule> for DbTagValueRule {
    fn from(value: TagValueRule) -> Self {
        Self(value)
    }
}

impl From<DbTagValueRule> for TagValueRule {
    fn from(value: DbTagValueRule) -> Self {
        value.0
    }
}

impl Load for DbTagValueRule {
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
            .map(Self)
            .map_err(|error| toasty::Error::invalid_result(error.to_string()))
    }

    fn reload(target: &mut Self, value: Value) -> toasty::Result<()> {
        *target = Self::load(value)?;
        Ok(())
    }
}

impl IntoExpr<DbTagValueRule> for DbTagValueRule {
    fn into_expr(self) -> Expr<DbTagValueRule> {
        self.by_ref()
    }

    fn by_ref(&self) -> Expr<DbTagValueRule> {
        let encoded = serde_json::to_string(&(self.0.allows_value(), self.0.value_regex()))
            .expect("tag value rule is serializable");
        Expr::from_untyped(Value::String(encoded))
    }
}

impl Assign<DbTagValueRule> for DbTagValueRule {
    fn into_assignment(self) -> Assignment<DbTagValueRule> {
        stmt::set(self.into_expr())
    }
}

impl Field for DbTagValueRule {
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

    fn new_update<'a>(
        _: &'a mut toasty_core::stmt::Assignments,
        _: stmt::Projection,
    ) -> Self::Update<'a> {
    }

    fn key_constraint<Origin>(&self, target: Path<Origin, Self::Inner>) -> Expr<bool> {
        target.eq(self.clone())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DbTimestamp(pub Timestamp);

impl From<Timestamp> for DbTimestamp {
    fn from(value: Timestamp) -> Self {
        Self(value)
    }
}

impl From<DbTimestamp> for Timestamp {
    fn from(value: DbTimestamp) -> Self {
        value.0
    }
}

impl Load for DbTimestamp {
    type Output = Self;

    fn ty() -> stmt::Type {
        stmt::Type::String
    }

    fn load(value: Value) -> toasty::Result<Self> {
        let Value::String(encoded) = value else {
            return Err(toasty::Error::invalid_result("timestamp is not text"));
        };
        encoded
            .parse::<Timestamp>()
            .map(Self)
            .map_err(|error| toasty::Error::invalid_result(error.to_string()))
    }

    fn reload(target: &mut Self, value: Value) -> toasty::Result<()> {
        *target = Self::load(value)?;
        Ok(())
    }
}

impl IntoExpr<DbTimestamp> for DbTimestamp {
    fn into_expr(self) -> Expr<DbTimestamp> {
        self.by_ref()
    }

    fn by_ref(&self) -> Expr<DbTimestamp> {
        Expr::from_untyped(Value::String(format!("{:.9}", self.0)))
    }
}

impl Assign<DbTimestamp> for DbTimestamp {
    fn into_assignment(self) -> Assignment<DbTimestamp> {
        stmt::set(self.into_expr())
    }
}

impl Field for DbTimestamp {
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

    fn new_update<'a>(
        _: &'a mut toasty_core::stmt::Assignments,
        _: stmt::Projection,
    ) -> Self::Update<'a> {
    }

    fn key_constraint<Origin>(&self, target: Path<Origin, Self::Inner>) -> Expr<bool> {
        target.eq(*self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbBookAuthors(pub Vec<BookAuthor>);

impl From<Vec<BookAuthor>> for DbBookAuthors {
    fn from(value: Vec<BookAuthor>) -> Self {
        Self(value)
    }
}

impl From<DbBookAuthors> for Vec<BookAuthor> {
    fn from(value: DbBookAuthors) -> Self {
        value.0
    }
}

impl Load for DbBookAuthors {
    type Output = Self;

    fn ty() -> stmt::Type {
        stmt::Type::String
    }

    fn load(value: Value) -> toasty::Result<Self> {
        let Value::String(encoded) = value else {
            return Err(toasty::Error::invalid_result("book authors is not text"));
        };
        let raw: Vec<String> = serde_json::from_str(&encoded)
            .map_err(|error| toasty::Error::invalid_result(error.to_string()))?;
        if raw.is_empty() {
            return Ok(Self(Vec::new()));
        }
        BookAuthor::parse_list(raw)
            .map(Self)
            .map_err(|error| toasty::Error::invalid_result(error.to_string()))
    }

    fn reload(target: &mut Self, value: Value) -> toasty::Result<()> {
        *target = Self::load(value)?;
        Ok(())
    }
}

impl IntoExpr<DbBookAuthors> for DbBookAuthors {
    fn into_expr(self) -> Expr<DbBookAuthors> {
        self.by_ref()
    }

    fn by_ref(&self) -> Expr<DbBookAuthors> {
        let names: Vec<&str> = self.0.iter().map(BookAuthor::as_str).collect();
        let encoded = serde_json::to_string(&names).expect("book authors are serializable");
        Expr::from_untyped(Value::String(encoded))
    }
}

impl Assign<DbBookAuthors> for DbBookAuthors {
    fn into_assignment(self) -> Assignment<DbBookAuthors> {
        stmt::set(self.into_expr())
    }
}

impl Field for DbBookAuthors {
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

    fn new_update<'a>(
        _: &'a mut toasty_core::stmt::Assignments,
        _: stmt::Projection,
    ) -> Self::Update<'a> {
    }

    fn key_constraint<Origin>(&self, target: Path<Origin, Self::Inner>) -> Expr<bool> {
        target.eq(self.clone())
    }
}
