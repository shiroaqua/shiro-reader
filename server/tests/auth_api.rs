mod support;
use support::*;
use support::crypto::{invalid_token, InvalidToken::*};
use HttpError::*;

// 验证用户名登录区分大小写，并支持 Unicode 用户名。
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

// 验证错误的登录凭据和非法用户名返回对应的错误。
#[tokio::test]
async fn bad_credentials_and_invalid_names_have_expected_errors() {
    let app = TestApp::new().await;
    app.register("Reader").await.assert_ok();
    app.attempt_login("Reader", "wrong").await.assert_error(InvalidCredentials);
    app.attempt_login("Unknown", PASSWORD).await.assert_error(InvalidCredentials);
    app.attempt_login("Bad Name", PASSWORD).await.assert_error(InvalidUsername);
    app.attempt_login("", PASSWORD).await.assert_error(MissingUsername);
}

// 验证刷新令牌可重复使用，且刷新后保留用户身份和书籍访问权限。
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

// 验证访问令牌和刷新令牌不能互换使用。
#[tokio::test]
async fn access_and_refresh_cannot_be_interchanged() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    app.refresh_with_access(&reader).await.assert_error(WrongTokenType);
    app.access_with_refresh(&reader).await.assert_error(WrongTokenType);
}

// 验证所有书库操作都要求身份认证。
#[tokio::test]
async fn every_library_operation_requires_authentication() {
    let app = TestApp::new().await;
    for response in app.anonymous_library_requests().await {
        response.assert_error(MissingBearer);
    }
}

// 验证认证头必须提供 Bearer 令牌，且 Bearer 标识不区分大小写。
#[tokio::test]
async fn bearer_scheme_is_case_insensitive_but_required() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    for header in [None, Some("Bearer"), Some("Bearer "), Some("Basic abc"), Some("Token abc")] {
        app.access_with_header(header).await.assert_error(MissingBearer);
    }
    app.access_with_mixed_case_bearer(&reader).await.assert_status(200);
}

// 验证各类无效令牌统一返回不暴露校验细节的 HTTP 错误。
#[tokio::test]
async fn invalid_tokens_have_sanitized_http_errors() {
    let app = TestApp::new().await;
    for case in [Malformed, Expired, Future, WrongIssuer, WrongAudience, MissingExpiry, WrongSignature, WrongAlgorithm] {
        app.access_with_token(&invalid_token(case, false)).await.assert_error(HttpError::InvalidToken);
        app.refresh_token(&invalid_token(case, true)).await.assert_error(HttpError::InvalidToken);
    }
}

// 验证格式错误的登录和令牌刷新请求返回预期状态码。
#[tokio::test]
async fn malformed_login_and_refresh_requests_are_rejected() {
    let app = TestApp::new().await;
    for (response, expected_status) in app.malformed_auth_requests().await {
        response.assert_status(expected_status);
    }
}

// 验证重复登出具有幂等性，整个会话失效且其他登录不受影响。
#[tokio::test]
async fn logout_revokes_the_whole_session_but_preserves_other_logins() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    let refreshed = app.refresh(&reader).await;
    let other_device = app.login("Reader", PASSWORD).await;
    let other_user = app.session("Other").await;

    app.logout(&reader).await.assert_empty(204);
    app.logout(&reader).await.assert_empty(204);
    app.access_books(&reader).await.assert_error(HttpError::InvalidToken);
    app.access_books(&refreshed).await.assert_error(HttpError::InvalidToken);
    app.attempt_refresh(&reader).await.assert_error(HttpError::InvalidToken);
    app.access_books(&other_device).await.assert_status(200);
    app.attempt_refresh(&other_device).await.assert_status(200);
    app.access_books(&other_user).await.assert_status(200);
}

// 验证错误类型或无效的登出令牌被拒绝，且现有会话仍可使用。
#[tokio::test]
async fn invalid_logout_credentials_cannot_revoke_a_session() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    app.logout_with_access(&reader).await.assert_error(WrongTokenType);
    for case in [Malformed, Expired, Future, WrongIssuer, WrongAudience, MissingExpiry, WrongSignature, WrongAlgorithm] {
        app.logout_token(&invalid_token(case, true)).await.assert_error(HttpError::InvalidToken);
    }
    app.access_books(&reader).await.assert_status(200);
    app.attempt_refresh(&reader).await.assert_status(200);
}

// 验证签名有效但未登记的令牌无法访问或刷新，登出它也不影响现有会话。
#[tokio::test]
async fn signed_but_unregistered_tokens_are_not_accepted() {
    use support::crypto::{access_token, claims, sign, SECRET};
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    let claims = claims(&reader.user_id);
    app.access_with_token(&access_token(&claims)).await.assert_error(HttpError::InvalidToken);
    let refresh = sign(&claims, Some("refresh+jwt"), jsonwebtoken::Algorithm::HS256, SECRET);
    app.refresh_token(&refresh).await.assert_error(HttpError::InvalidToken);
    app.logout_token(&refresh).await.assert_empty(204);
    app.access_books(&reader).await.assert_status(200);
}

// 验证应用重建后已登出的会话仍然失效，其他会话仍可访问。
#[tokio::test]
async fn logout_persists_across_application_rebuilds() {
    let root = tempfile::tempdir().unwrap();
    let app = TestApp::at(root.path()).await;
    let reader = app.session("Reader").await;
    let other = app.login("Reader", PASSWORD).await;
    app.logout(&reader).await.assert_empty(204);
    drop(app);
    let app = TestApp::at(root.path()).await;
    app.access_books(&reader).await.assert_error(HttpError::InvalidToken);
    app.attempt_refresh(&reader).await.assert_error(HttpError::InvalidToken);
    app.access_books(&other).await.assert_status(200);
}
