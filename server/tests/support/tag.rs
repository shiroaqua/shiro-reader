use super::*;

const TAGS: &str = "/api/v1/library/bookshelf/tags";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TagDefinitionData {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub allows_value: bool,
    pub value_regex: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TagNode {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub allows_value: bool,
    pub value_regex: Option<String>,
    pub children: Vec<TagNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BookTagData {
    pub tag_id: String,
    pub value: Option<String>,
}

impl TagDefinitionData {
    pub fn binding(&self, value: Option<&str>) -> BookTagData {
        BookTagData { tag_id: self.id.clone(), value: value.map(str::to_owned) }
    }

    pub fn node(&self, children: Vec<TagNode>) -> TagNode {
        TagNode {
            id: self.id.clone(), name: self.name.clone(), parent_id: self.parent_id.clone(),
            allows_value: self.allows_value, value_regex: self.value_regex.clone(), children,
        }
    }

    pub fn under(&self, parent: Option<&str>) -> Self {
        Self { parent_id: parent.map(str::to_owned), ..self.clone() }
    }

    pub fn renamed(&self, name: &str) -> Self {
        Self { name: name.into(), ..self.clone() }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum TagError {
    InvalidId, MissingName, InvalidName, UnchangedName, InvalidRegex, RegexNotAllowed,
    ValueRequired, ValueNotAllowed, RegexMismatch, NotFound, ParentNotFound,
    InvalidRequest, InvalidStrategy,
}

impl TagError {
    fn contract(self) -> (u16, &'static str) {
        use TagError::*;
        match self {
            InvalidId => (400, "library.tag.invalid_id"),
            MissingName => (400, "library.tag.missing_name"),
            InvalidName => (400, "library.tag.invalid_name_format"),
            UnchangedName => (400, "library.tag.name_unchanged"),
            InvalidRegex => (400, "library.tag.invalid_value_regex"),
            RegexNotAllowed => (400, "library.tag.value_regex_not_allowed"),
            ValueRequired => (400, "library.tag.value_required"),
            ValueNotAllowed => (400, "library.tag.value_not_allowed"),
            RegexMismatch => (400, "library.tag.value_regex_mismatch"),
            NotFound => (404, "library.tag.not_found"),
            ParentNotFound => (404, "library.tag.parent_not_found"),
            InvalidRequest => (400, "library.tag.invalid_request"),
            InvalidStrategy => (400, "library.tag.invalid_delete_strategy"),
        }
    }
}

impl HttpResponse {
    pub fn assert_tag_error(&self, error: TagError) {
        let (status, message) = error.contract();
        self.assert_status(status);
        assert_eq!(self.json(), json!({"error": {"message": message}}));
    }


}

#[derive(Clone, Copy)]
pub enum DeleteStrategy { Cascade, Promote }

impl DeleteStrategy {
    fn query(self) -> &'static str {
        match self { Self::Cascade => "?strategy=CASCADE", Self::Promote => "?strategy=PROMOTE" }
    }
}

impl TestApp {
    pub async fn query_book_tags(&self, session: &Session, book_id: &str) -> HttpResponse {
        self.send(Method::GET, &format!("{BOOKS}/{book_id}/tags"), None, Some(session)).await
    }

    pub async fn book_tags(&self, session: &Session, book_id: &str) -> Vec<BookTagData> {
        let response = self.query_book_tags(session, book_id).await;
        response.assert_status(200);
        serde_json::from_value(response.json()).expect("book tag fields")
    }

    pub async fn assert_book_tags(&self, session: &Session, book_id: &str, expected: &[BookTagData]) {
        let actual = self.book_tags(session, book_id).await;
        let mut expected = expected.to_vec();
        expected.sort_by(|a, b| a.tag_id.cmp(&b.tag_id));
        assert_eq!(actual, expected);
    }

    pub async fn define_tag(&self, session: &Session, name: &str, parent: Option<&str>,
        allows_value: bool, regex: Option<&str>) -> HttpResponse {
        self.send(Method::POST, TAGS, Some(json!({
            "name": name, "parent_id": parent, "allows_value": allows_value, "value_regex": regex,
        })), Some(session)).await
    }

    pub async fn add_tag(&self, session: &Session, name: &str, parent: Option<&str>,
        allows_value: bool, regex: Option<&str>) -> TagDefinitionData {
        let response = self.define_tag(session, name, parent, allows_value, regex).await;
        response.assert_status(201);
        let tag: TagDefinitionData = serde_json::from_value(response.json()).expect("tag definition fields");
        uuid::Uuid::parse_str(&tag.id).expect("tag definition ID");
        assert_eq!(tag.name, name);
        assert_eq!(tag.parent_id.as_deref(), parent);
        assert_eq!(tag.allows_value, allows_value);
        assert_eq!(tag.value_regex.as_deref(), regex.filter(|pattern| !pattern.is_empty()));
        tag
    }

    pub async fn tag_tree(&self, session: &Session) -> Vec<TagNode> {
        let response = self.send(Method::GET, TAGS, None, Some(session)).await;
        response.assert_status(200);
        serde_json::from_value(response.json()).expect("tag tree fields")
    }

    pub async fn rename_tag(&self, session: &Session, id: &str, name: &str) -> HttpResponse {
        self.send(Method::PATCH, &format!("{TAGS}/{id}"), Some(json!({"name": name})), Some(session)).await
    }

    pub async fn delete_tag(&self, session: &Session, id: &str, strategy: DeleteStrategy) -> HttpResponse {
        self.send(Method::DELETE, &format!("{TAGS}/{id}{}", strategy.query()), None, Some(session)).await
    }

    pub async fn bind_tag(&self, session: &Session, book_id: &str, tag_id: &str, value: Option<&str>) -> HttpResponse {
        let mut body = json!({"tag_id": tag_id});
        if let Some(value) = value { body["value"] = json!(value); }
        self.send(Method::POST, &format!("{BOOKS}/{book_id}/tags"), Some(body), Some(session)).await
    }

    pub async fn unbind_tag(&self, session: &Session, book_id: &str, tag_id: &str) -> HttpResponse {
        self.send(Method::DELETE, &format!("{BOOKS}/{book_id}/tags"),
            Some(json!({"tag_id": tag_id})), Some(session)).await
    }

    pub async fn forbidden_tag_definition_changes(&self, session: &Session, id: &str) -> Vec<HttpResponse> {
        let mut responses = Vec::new();
        for field in [
            json!({"id": UNKNOWN_ID}), json!({"parent_id": UNKNOWN_ID}),
            json!({"allows_value": false}), json!({"value_regex": ".*"}), json!({"color": "red"}),
        ] {
            let mut body = json!({"name": "Changed"});
            body.as_object_mut().unwrap().extend(field.as_object().unwrap().clone());
            responses.push(self.send(Method::PATCH, &format!("{TAGS}/{id}"), Some(body), Some(session)).await);
        }
        responses
    }

    pub async fn malformed_tag_requests(&self, session: &Session, tag_id: &str, book_id: &str) -> Vec<HttpResponse> {
        let mut responses = Vec::new();
        for (method, uri, body) in [
            (Method::POST, TAGS.into(), json!({"name": "Tag"})),
            (Method::POST, TAGS.into(), json!({"name": "Tag", "allows_value": "true"})),
            (Method::POST, TAGS.into(), json!({"name": "Tag", "allows_value": false, "color": "red"})),
            (Method::PATCH, format!("{TAGS}/{tag_id}"), json!({"name": 42})),
            (Method::POST, format!("{BOOKS}/{book_id}/tags"), json!({"tag_id": tag_id, "value": 42})),
            (Method::POST, format!("{BOOKS}/{book_id}/tags"), json!({"tag_id": tag_id, "value": []})),
            (Method::POST, format!("{BOOKS}/{book_id}/tags"), json!({"tags": [{"tag_id": tag_id}]})),
            (Method::DELETE, format!("{BOOKS}/{book_id}/tags"), json!({"tag_ids": [tag_id]})),
        ] {
            responses.push(self.send(method, &uri, Some(body), Some(session)).await);
        }
        responses.push(self.request(Method::POST, TAGS, Body::from("{"), Some("application/json"),
            Some(&format!("Bearer {}", session.access_token))).await);
        responses
    }

    pub async fn invalid_tag_deletion_strategies(&self, session: &Session, id: &str) -> Vec<HttpResponse> {
        let mut responses = Vec::new();
        for query in ["", "?strategy=", "?strategy=cascade", "?strategy=UNKNOWN"] {
            responses.push(self.send(Method::DELETE, &format!("{TAGS}/{id}{query}"), None, Some(session)).await);
        }
        responses
    }

    pub async fn anonymous_tag_requests(&self) -> Vec<HttpResponse> {
        let mut responses = Vec::new();
        for (method, uri, body) in [
            (Method::GET, TAGS.into(), None),
            (Method::GET, format!("{BOOKS}/{UNKNOWN_ID}/tags"), None),
            (Method::POST, TAGS.into(), Some(json!({"name": "Tag", "allows_value": false}))),
            (Method::PATCH, format!("{TAGS}/{UNKNOWN_ID}"), Some(json!({"name": "Renamed"}))),
            (Method::DELETE, format!("{TAGS}/{UNKNOWN_ID}?strategy=CASCADE"), None),
            (Method::POST, format!("{BOOKS}/{UNKNOWN_ID}/tags"), Some(json!({"tag_id": UNKNOWN_ID}))),
            (Method::DELETE, format!("{BOOKS}/{UNKNOWN_ID}/tags"), Some(json!({"tag_id": UNKNOWN_ID}))),
        ] {
            responses.push(self.send(method, &uri, body, None).await);
        }
        responses
    }
}

