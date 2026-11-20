use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::header;
use axum::response::{Html, IntoResponse, Response};
use bytes::BytesMut;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;

use super::{wait_for_samples, PcmTap};
use crate::audio::encoder::f32_to_s16le;
use crate::engine::Engine;

const PAGE: &str = include_str!("../../web/receiver.html");
const WORKLET: &str = include_str!("../../web/worklet.js");

const CHUNK_SAMPLES: usize = 480;

pub async fn page() -> Html<&'static str> {
    Html(PAGE)
}

pub async fn worklet() -> Response {
    ([(header::CONTENT_TYPE, "application/javascript; charset=utf-8"), (header::CACHE_CONTROL, "no-cache")], WORKLET)
        .into_response()
}

#[derive(Deserialize)]
pub struct WsQuery {
    id: String,
    name: Option<String>,
}

pub async fn ws(ws: WebSocketUpgrade, Query(q): Query<WsQuery>, State(engine): State<Arc<Engine>>) -> Response {
    ws.on_upgrade(move |socket| session(engine, q, socket))
}

fn sanitize_id(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').take(40).collect()
}

async fn session(engine: Arc<Engine>, q: WsQuery, socket: WebSocket) {
    let id = sanitize_id(&q.id);
    if id.is_empty() {
        return;
    }
    let name = q.name.map(|n| n.chars().take(40).collect::<String>()).filter(|n| !n.trim().is_empty()).unwrap_or_else(|| "Appareil Web".into());
    let (mut tx, mut rx) = socket.split();
    let token = engine.web_attach(&id, &name);
    let speaker_id = format!("web-{id}");

    let hello = serde_json::json!({ "t": "hello", "rate": 48000, "ch": 2 }).to_string();
    if tx.send(Message::Text(hello.into())).await.is_err() {
        engine.web_detach(&id, token);
        return;
    }

    let eng2 = engine.clone();
    let id2 = id.clone();
    let reader = tokio::spawn(async move {
        while let Some(Ok(m)) = rx.next().await {
            match m {
                Message::Text(t) => {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(t.as_str()) {
                        if v["t"] == "stats" {
                            if let Some(ms) = v["buf"].as_f64() {
                                eng2.web_stats(&id2, ms.max(0.0) as u32);
                            }
                        }
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });

    let mut cur: Option<(Arc<PcmTap>, u64)> = None;
    let mut scratch: Vec<f32> = Vec::with_capacity(CHUNK_SAMPLES);
    let mut bytes = BytesMut::with_capacity(CHUNK_SAMPLES * 2 * 8);

    let mut reason = "client parti";
    loop {
        if reader.is_finished() {
            break;
        }
        let Some(tap) = engine.tap(&speaker_id) else {
            cur = None;
            tokio::time::sleep(Duration::from_millis(20)).await;
            continue;
        };
        let fresh = cur.as_ref().map_or(true, |(c, _)| !Arc::ptr_eq(c, &tap));
        if fresh {
            let g = tap.gen.fetch_add(1, Ordering::AcqRel) + 1;
            tap.drain();
            tap.connected.store(true, Ordering::Release);
            engine.notify_changed();
            cur = Some((tap.clone(), g));
        }
        let (tap, my_gen) = cur.as_ref().unwrap();
        if tap.gen.load(Ordering::Acquire) != *my_gen {
            reason = "session plus récente";
            break;
        }

        let avail = tap.available();
        if avail < CHUNK_SAMPLES {
            wait_for_samples(tap, CHUNK_SAMPLES, Duration::from_millis(20)).await;
            continue;
        }
        if avail > 4800 {
            scratch.clear();
            tap.read_into(&mut scratch, avail - 1920);
        }
        scratch.clear();
        tap.read_into(&mut scratch, CHUNK_SAMPLES);
        bytes.reserve(CHUNK_SAMPLES * 2);
        f32_to_s16le(&scratch, &mut bytes);
        if tx.send(Message::Binary(bytes.split().freeze())).await.is_err() {
            reason = "envoi impossible";
            break;
        }
    }
    log::info!("récepteur Web {id} : fin de session ({reason})");

    reader.abort();
    if let Some((tap, g)) = &cur {
        if tap.gen.load(Ordering::Acquire) == *g {
            tap.connected.store(false, Ordering::Release);
        }
    }
    engine.web_detach(&id, token);
}
