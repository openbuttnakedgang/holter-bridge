use ellocopo2::AnswerCode;
use ellocopo2::ParseMsg;
use ellocopo2::RequestBuilder;
use futures::channel::mpsc;
use futures::prelude::*;
use std::net::SocketAddr;
use tokio::runtime::Runtime;
//use ellocopo::OperationStatus;
use std::collections::HashMap;

//use crate::parser::pars_answer;
use tokio::sync::{mpsc as t_mpsc, Mutex};
use warp::ws::{Message, WebSocket};
use warp::{self, Filter, Rejection};

use std::sync::{atomic::AtomicBool, Arc};

#[macro_use]
extern crate log;

//mod error;
mod usb;
mod usbfutures;
mod web;
//mod parser;

use usb::USBDevices;

pub struct GlobalState {
    pub vis: Arc<AtomicBool>,
}

impl GlobalState {
    pub fn new() -> Self {
        Self {
            vis: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Clone for GlobalState {
    fn clone(&self) -> Self {
        Self {
            vis: self.vis.clone(),
        }
    }
}

async fn send_command(usb_devices: USBDevices, msg: ellocopo2::owned::Msg) -> Result<(), String> {
    let usb_devices = usb_devices.clone();
    tokio::time::delay_for(std::time::Duration::from_secs(1)).await;
    let list = usb_devices.devices().await;
    info!("List of devices: {:#?}", &list);
    assert!(list.len() > 0, "No devices in list");

    let dev = usb_devices.acquire_device(&list[0]["path"]).await;
    if let Ok(Some((mut tx, mut rx))) = dev {
        tx.send(msg).await.expect("Echo: cant not send");
        match rx.next().await {
            Some(r) => {
                let ellocopo2::owned::Msg(code, path, value) = r;
                info!("Finally here!: {:?}", value);
                return Ok(());
            }
            None => Err("Echo: no recive data".to_string()),
        }
    } else {
        error!("Echo: no device channels");
        return Err("Echo: no device channels".to_string());
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Check if the user requested some specific log level via an env variable. Otherwise set log
    // level to something reasonable.
    if let Ok(_) = std::env::var("RUST_LOG") {
        env_logger::init();
    } else {
        env_logger::Builder::new()
            .filter_level(log::LevelFilter::Debug)
            .init();
    }
    println!("Set RUST_LOG=<filter> to enable logging. Example RUST_LOG=debug");

    // Create an async runtime for spawning futures on
    let mut rt = Runtime::new()?;

    // Create the global state that can be shared between threads
    let usb_devices = USBDevices::new()?;

    // Create a channel with which it is possible to request a refresh of usb devices. A length of
    // 1 is enough since it doesn't make sense to request more refreses than the refresh task can
    // execute.
    let (mut notify_tx, notify_rx) = mpsc::channel(1);
    // Trigger one refresh on startup
    //web::notify(&mut notify_tx);
    notify_tx.try_send(()).expect("Startup trigger not worked");

    // Create and spawn the future that polls for USB devices
    let usb_poller = {
        let usb_devices = usb_devices.clone();
        async move {
            if let Err(e) = usb_devices.presence_detector(notify_rx).await {
                error!("Stopped polling for usb devices: {}", e);
            }
        }
    };

    let echo = {
        let usb_devices = usb_devices.clone();
        let msg = ellocopo2::owned::Msg(
            AnswerCode::OK_WRITE,
            "/ctrl/vis".to_string(),
            ellocopo2::owned::Value::BOOL(true),
        );
        let msg1 = ellocopo2::owned::Msg(
            AnswerCode::OK_READ,
            "/ctrl/vis".to_string(),
            ellocopo2::owned::Value::UNIT(()),
        );
        async move {
            send_command(usb_devices.clone(), msg).await;
            send_command(usb_devices.clone(), msg1).await;
            /*tokio::time::delay_for(std::time::Duration::from_secs(1)).await;
            let list = usb_devices.devices().await;
            info!("List of devices: {:#?}", &list);
            assert!(list.len() > 0, "No devices in list");

            let dev = usb_devices.acquire_device(&list[0]["path"]).await;
            if let Ok(Some((mut tx, mut rx))) = dev {
                let msg = ellocopo2::owned::Msg(AnswerCode::OK_READ, "/survey/surname".to_string(), ellocopo2::owned::Value::UNIT(()));
                tx.send(msg).await.expect("Echo: cant not send");
                match rx.next().await {
                    Some(r) => {
                        let ellocopo2::owned::Msg (code, path, value) = r;
                        info!("Finally here!: {:?}", value);
                    }
                    None => error!("Echo: no recive data"),
                }
            }
            else { error!("Echo: no device channels") }*/
        }
    };

    let state: GlobalState = GlobalState::new();

    let addr: SocketAddr = "127.0.0.1:3333".parse()?;

    println!("listening on http://{}", addr);

    let server = web::create(addr, state.clone(), usb_devices);

    rt.block_on(async move {
        tokio::select! {
            _ = server => info!("Warp returned"),
            _ = usb_poller => info!("Usb poller died"),
            //_ = echo => info!("Echo ended"),
        }
    });

    Ok(())
}

#[cfg(test)]
mod tests {

    use ellocopo2::Value;
    use serde::{Deserialize, Serialize};

    #[test]
    fn test1() {
        let val = Value::STR("Hello world");
        let j = serde_json::to_string(&val).unwrap();
        println!("{}", &j);

        let v: Value = serde_json::from_str(&j).unwrap();
        println!("{:?}", v);

        let val = Value::BYTES(&[1, 2, 3, 4]);
        let j = serde_json::to_string(&val);
        println!("{}", j.unwrap());

        let val = Value::BOOL(true);
        let j = serde_json::to_string(&val);
        println!("{}", j.unwrap());

        let val = Value::U32(7);
        let j = serde_json::to_string(&val);
        println!("{}", j.unwrap());

        let val = Value::UNIT(());
        let j = serde_json::to_string(&val).unwrap();
        println!("{}", &j);
    }
}
