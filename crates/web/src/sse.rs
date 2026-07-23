use std::convert::Infallible;
use std::time::Duration;

use axum::extract::State;
use axum::http::header;
use axum::http::HeaderValue;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::Stream;
use tokio::sync::broadcast::error::RecvError;

use crate::state::AppState;

/// GET /api/events — stream LogEvent as SSE (`event: <channel>\ndata: <message>`).
pub async fn events_handler(
    State(state): State<AppState>,
) -> impl axum::response::IntoResponse {
    let rx = state.ctx.subscribe_logs();

    let stream = futures::stream::unfold(rx, |mut rx| async move {
        match rx.recv().await {
            Ok(ev) => {
                let event = Event::default().event(ev.channel).data(ev.message);
                Some((Ok::<_, Infallible>(event), rx))
            }
            Err(RecvError::Lagged(_)) => {
                Some((Ok(Event::default().comment("lagged")), rx))
            }
            Err(RecvError::Closed) => None,
        }
    });

    let sse = Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    );

    (
        [
            (header::CACHE_CONTROL, HeaderValue::from_static("no-cache, no-transform")),
            (header::CONNECTION, HeaderValue::from_static("keep-alive")),
            (
                header::HeaderName::from_static("x-accel-buffering"),
                HeaderValue::from_static("no"),
            ),
        ],
        sse,
    )
}
