use futures_util::TryStreamExt;
use shiro_compatibility::ClientRuntime;
use shiro_error_catalog::ErrorCode;
use tauri::http::Response;
use tracing::warn;

/// Maximum cover image payload returned via the `shiro-cover` URI scheme protocol.
const MAX_COVER_BYTES: usize = 32 * 1024 * 1024;

fn plain_text_response(status: u16, body: &'static [u8]) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header("Content-Type", "text/plain; charset=utf-8")
        .header("Cache-Control", "no-store")
        .body(body.to_vec())
        .unwrap_or_else(|_| Response::new(Vec::new()))
}

pub async fn handle_cover_request(runtime: &ClientRuntime, path: &str) -> Response<Vec<u8>> {
    let Some(book_id) = path
        .strip_prefix("/book/")
        .map(|segment| segment.trim_matches('/'))
        .filter(|segment| !segment.is_empty() && !segment.contains('/'))
    else {
        return plain_text_response(404, b"book not found");
    };

    let (mut stream, mime) = match runtime.get_cover_stream(book_id.to_owned()).await {
        Ok(result) => result,
        Err(
            ErrorCode::SetupRequired
            | ErrorCode::RuntimeNotReady
            | ErrorCode::CloudProtocolIncompatible
            | ErrorCode::CloudNetworkUnreachable
            | ErrorCode::CloudRequestTimeout,
        ) => {
            return plain_text_response(503, b"library unavailable");
        }
        Err(ErrorCode::CloudNotAuthenticated | ErrorCode::AuthenticationInvalidToken) => {
            return plain_text_response(401, b"cloud session unavailable");
        }
        Err(code) => {
            warn!(book_id, code = code.code(), "cover request failed");
            return plain_text_response(404, b"cover unavailable");
        }
    };

    let mut bytes = Vec::new();
    loop {
        match stream.try_next().await {
            Ok(Some(chunk)) => {
                if bytes.len().saturating_add(chunk.len()) > MAX_COVER_BYTES {
                    warn!(book_id, "cover payload exceeded maximum response limit");
                    return plain_text_response(413, b"cover too large");
                }
                bytes.extend_from_slice(&chunk);
            }
            Ok(None) => break,
            Err(err) => {
                warn!(book_id, error = %err, "cover stream read failed");
                return plain_text_response(500, b"cover read failed");
            }
        }
    }

    Response::builder()
        .status(200)
        .header("Content-Type", mime)
        .header("Cache-Control", "no-store")
        .body(bytes)
        .unwrap_or_else(|_| plain_text_response(500, b"failed to build cover response"))
}
