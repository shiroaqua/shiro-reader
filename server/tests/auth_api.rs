mod support;
use support::*;
use support::crypto::{invalid_token, InvalidToken::*};
use HttpError::*;

#[tokio::test]
async fn username_login_is_exact_and_supports_unicode() {
    let app = TestApp::new().await;
    let upper = app.session("Reader").await;
    let lower = app.session("reader").await;
    let unicode = app.session("读者A").await;
    assert_ne!(upper.user_id, lower.user_id);
    assert_eq!(app.login("Reader", PASSWORD).await.user_id, upper.user_id);
    assert_eq!(app.login("reader", PASSWORD).await.user_id, lower.user_id);
    app.access_books(&unicode).await.assert_status(200);
    app.attempt_login("READER", PASSWORD).await.assert_error(InvalidCredentials);
}

#[tokio::test]
async fn bad_credentials_and_invalid_names_have_expected_errors() {
    let app = TestApp::new().await;
    app.register("Reader").await.assert_ok();
    app.attempt_login("Reader", "wrong").await.assert_error(InvalidCredentials);
    app.attempt_login("Unknown", PASSWORD).await.assert_error(InvalidCredentials);
    app.attempt_login("Bad Name", PASSWORD).await.assert_error(InvalidUsername);
    app.attempt_login("", PASSWORD).await.assert_error(MissingUsername);
}

#[tokio::test]
async fn refresh_preserves_identity_and_book_access_and_can_be_reused() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    app.upload(&reader, SampleFile::Epub).await;
    let book = app.add_book(&reader, "Private", SampleFile::Epub).await;
    for _ in 0..2 {
        let refreshed = app.refresh(&reader).await;
        assert_eq!(refreshed.user_id, reader.user_id);
        assert_eq!(app.books(&refreshed).await, [book.data.clone()]);
    }
}

#[tokio::test]
async fn access_and_refresh_cannot_be_interchanged() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    app.refresh_with_access(&reader).await.assert_error(WrongTokenType);
    app.access_with_refresh(&reader).await.assert_error(WrongTokenType);
}

#[tokio::test]
async fn every_library_operation_requires_authentication() {
    let app = TestApp::new().await;
    for response in app.anonymous_library_requests().await {
        response.assert_error(MissingBearer);
    }
}

#[tokio::test]
async fn bearer_scheme_is_case_insensitive_but_required() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    for header in [None, Some("Bearer"), Some("Bearer "), Some("Basic abc"), Some("Token abc")] {
        app.access_with_header(header).await.assert_error(MissingBearer);
    }
    app.access_with_mixed_case_bearer(&reader).await.assert_status(200);
}

#[tokio::test]
async fn invalid_tokens_have_sanitized_http_errors() {
    let app = TestApp::new().await;
    for case in [Malformed, Expired, Future, WrongIssuer, WrongAudience, MissingExpiry, WrongSignature, WrongAlgorithm] {
        app.access_with_token(&invalid_token(case, false)).await.assert_error(HttpError::InvalidToken);
        app.refresh_token(&invalid_token(case, true)).await.assert_error(HttpError::InvalidToken);
    }
}

#[tokio::test]
async fn malformed_login_and_refresh_requests_are_rejected() {
    let app = TestApp::new().await;
    for (response, expected_status) in app.malformed_auth_requests().await {
        response.assert_status(expected_status);
    }
}
