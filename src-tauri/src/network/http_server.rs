use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use axum::routing::{any, get};
use axum::serve::ListenerExt;
use axum::Router;
use tokio::net::TcpListener;

use super::{sonos_upnp, web_receiver};
use crate::engine::Engine;

pub async fn serve(engine: Arc<Engine>, preferred: u16) -> Result<u16> {
    let listener = match TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], preferred))).await {
        Ok(l) => l,
        Err(e) => {
            log::warn!("port {preferred} indisponible ({e}) : port aléatoire");
            TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], 0))).await?
        }
    };
    let port = listener.local_addr()?.port();

    let app = Router::new()
        .route("/r/worklet.js", get(web_receiver::worklet))
        .route("/r/{key}", get(web_receiver::page))
        .route("/ws", get(web_receiver::ws))
        .route("/s/{key}/stream/{file}", get(sonos_upnp::wav_handler))
        .route("/s/{key}/event/{id}", any(sonos_upnp::event_handler))
        .with_state(engine);

    let listener = listener.tap_io(|tcp| {
        let _ = tcp.set_nodelay(true);
    });

    tauri::async_runtime::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            log::error!("serveur HTTP arrêté : {e}");
        }
    });
    log::info!("serveur HTTP en écoute sur le port {port}");
    Ok(port)
}
