use shiro_compatibility::ClientRuntime;
use tauri::http::{Method, Response};

fn plain_text_response(status: u16, body: &'static [u8]) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header("Content-Type", "text/plain; charset=utf-8")
        .header("Access-Control-Allow-Origin", "*")
        .header("Cache-Control", "no-store")
        .body(body.to_vec())
        .unwrap_or_else(|_| Response::new(Vec::new()))
}

pub async fn handle_font_request(
    runtime: &ClientRuntime,
    method: &Method,
    path: &str,
) -> Response<Vec<u8>> {
    if *method == Method::OPTIONS {
        return Response::builder()
            .status(204)
            .header("Access-Control-Allow-Origin", "*")
            .header("Access-Control-Allow-Methods", "GET, HEAD, OPTIONS")
            .header("Access-Control-Allow-Headers", "*")
            .header("Cache-Control", "public, max-age=86400")
            .body(Vec::new())
            .unwrap_or_else(|_| Response::new(Vec::new()));
    }

    if *method != Method::GET && *method != Method::HEAD {
        return plain_text_response(405, b"method not allowed");
    }

    let Some(font_id) = path
        .strip_prefix("/font/")
        .map(|segment| segment.trim_matches('/'))
        .filter(|segment| !segment.is_empty() && !segment.contains('/'))
    else {
        return plain_text_response(404, b"font not found");
    };

    let Ok((bytes, format)) = runtime.read_custom_font(font_id).await else {
        return plain_text_response(404, b"font not found");
    };

    let content_len = bytes.len();
    let body = if *method == Method::HEAD {
        Vec::new()
    } else {
        bytes
    };

    Response::builder()
        .status(200)
        .header("Content-Type", format.mime_type())
        .header("Content-Length", content_len.to_string())
        .header("Access-Control-Allow-Origin", "*")
        .header("Cache-Control", "public, max-age=31536000, immutable")
        .body(body)
        .unwrap_or_else(|_| plain_text_response(500, b"failed to build font response"))
}
