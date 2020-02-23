
extern crate pretty_env_logger;
#[macro_use] extern crate log;

use serde_derive::{Deserialize, Serialize};
use std::convert::Infallible;
use std::str::FromStr;
use std::time::Duration;
use warp::Filter;
use futures::{FutureExt, StreamExt};

#[tokio::main]
async fn main() {
    pretty_env_logger::init();
    info!("such information2");

    let routes = warp::path("echo")
        // The `ws()` filter will prepare the Websocket handshake.
        .and(warp::ws())
        .map(|ws: warp::ws::Ws| {
            // And then our closure will be called when it completes...
            ws.on_upgrade(|websocket| {
                // Just echo all messages back...
                let (tx, rx) = websocket.split();
                rx.forward(tx).map(|result| {
                    if let Err(e) = result {
                        eprintln!("websocket error: {:?}", e);
                    }
                })
            })
        });

    warp::serve(routes).run(([127, 0, 0, 1], 3030)).await;
}
