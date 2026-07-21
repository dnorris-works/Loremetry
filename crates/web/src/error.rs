use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};

pub fn json_error(msg: impl Into<String>) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "error": msg.into() })),
    )
        .into_response()
}

pub fn json_error_status(status: StatusCode, msg: impl Into<String>) -> Response {
    (
        status,
        Json(json!({ "error": msg.into() })),
    )
        .into_response()
}

pub fn ok_json(value: Value) -> Response {
    Json(value).into_response()
}

pub fn result_to_response<T: serde::Serialize>(result: Result<T, String>) -> Response {
    match result {
        Ok(v) => match serde_json::to_value(v) {
            Ok(val) => ok_json(val),
            Err(e) => json_error(format!("Serialize error: {e}")),
        },
        Err(e) => json_error(e),
    }
}
