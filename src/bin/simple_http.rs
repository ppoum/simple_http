use std::net::{TcpListener, TcpStream};

use anyhow::Context;
use http_lib::http::Request;
use log::{debug, info, log_enabled, Level};

fn main() -> anyhow::Result<()> {
    env_logger::init();
    start_server()?;

    Ok(())
}

fn start_server() -> anyhow::Result<()> {
    info!("Starting server on port 8080");

    let listener = TcpListener::bind("0.0.0.0:8080").context("Failed to start listener")?;

    for stream in listener.incoming() {
        handle_connection(stream?)?;
    }

    Ok(())
}

fn handle_connection(stream: TcpStream) -> anyhow::Result<()> {
    if log_enabled!(Level::Debug) {
        let peer_addr = stream.peer_addr().context("Failed to read peer address")?;
        debug!("Received a new connection from {}", peer_addr)
    };

    let request = Request::try_from(stream)?;
    debug!("{:?}", request);
    todo!()
}
