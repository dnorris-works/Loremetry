use std::convert::Infallible;
use std::pin::Pin;
use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::header;
use axum::http::HeaderValue;
use axum::response::sse::{Event, KeepAlive, KeepAliveStream, Sse};
use futures::stream::Stream;
use loremetry_core::jobs;
use serde::Deserialize;
use tokio::sync::broadcast::error::RecvError;
use uuid::Uuid;

use crate::state::AppState;

#[derive(Deserialize)]
pub struct EventsQuery {
    pub job_id: Option<Uuid>,
}

type EventStream = Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>;
type LogSse = Sse<KeepAliveStream<EventStream>>;

fn sse_headers() -> [(axum::http::HeaderName, HeaderValue); 3] {
    [
        (header::CACHE_CONTROL, HeaderValue::from_static("no-cache, no-transform")),
        (header::CONNECTION, HeaderValue::from_static("keep-alive")),
        (
            header::HeaderName::from_static("x-accel-buffering"),
            HeaderValue::from_static("no"),
        ),
    ]
}

fn wrap_sse(stream: EventStream) -> LogSse {
    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

/// GET /api/events — stream LogEvent as SSE (`event: <channel>\ndata: <message>`).
/// With `?job_id=`, streams persisted job log lines from the worker.
pub async fn events_handler(
    State(state): State<AppState>,
    Query(query): Query<EventsQuery>,
) -> impl axum::response::IntoResponse {
    let stream: EventStream = if let Some(job_id) = query.job_id {
        let pool = state.ctx.db.pool.clone();
        Box::pin(
            futures::stream::unfold(
                (0i64, Vec::<jobs::JobEvent>::new(), pool, job_id),
                |(after_id, mut pending, pool, job_id)| async move {
                    if pending.is_empty() {
                        tokio::time::sleep(Duration::from_millis(300)).await;
                        pending = jobs::fetch_events_since(&pool, job_id, after_id).await;
                        if pending.is_empty() {
                            let status = jobs::job_status(&pool, job_id).await;
                            match status.as_deref() {
                                Some("pending") | Some("running") => {
                                    return Some((
                                        Ok(Event::default().comment("keep-alive")),
                                        (after_id, pending, pool, job_id),
                                    ));
                                }
                                _ => return None,
                            }
                        }
                    }

                    let ev = pending.remove(0);
                    let next_after = ev.id;
                    let event = Event::default().event(ev.channel).data(ev.message);
                    Some((Ok(event), (next_after, pending, pool, job_id)))
                },
            ),
        )
    } else {
        let rx = state.ctx.subscribe_logs();
        Box::pin(futures::stream::unfold(rx, |mut rx| async move {
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
        }))
    };

    (sse_headers(), wrap_sse(stream))
}
