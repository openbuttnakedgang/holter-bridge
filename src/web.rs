use futures::prelude::*;
use std::collections::HashMap;
use std::fs;
use std::net::SocketAddr;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use tokio::sync::{mpsc, Mutex};
use warp::ws::{Message, WebSocket};
use warp::{self, Filter, Rejection};

use ellocopo2::AnswerCode;
use ellocopo2::owned::Msg;

use crate::GlobalState;
use crate::USBDevices;

fn commands_parse(arr: Vec<String>, commands: Vec<String>) -> Msg {
    let mut command = String::from("");
    for i in 0..arr.len() {
        command.push_str("/");
        command.push_str(&arr[i]);
    }
    for i in 0..commands.len() {
        if commands[i] == command {
            let cmd = command.clone();
            let msg = Msg(
                AnswerCode::OK_READ,
                cmd,
                ellocopo2::owned::Value::UNIT(()),
            );
            return msg;
        }
    }
    let msg = Msg(
        AnswerCode::OK_READ,
        "Wrong command".to_string(),
        ellocopo2::owned::Value::UNIT(()),
    );
    msg
}

async fn list_devices(usb_devices: USBDevices) -> Result<impl warp::Reply, Rejection> {
    let list = usb_devices.devices().await;
    info!("List of devices: {:#?}", &list);
    let mut map = HashMap::new();
    map.insert("devices", list);
    let reply = warp::reply::json(&map);
    Ok(reply)
}

async fn send_command_web(
    msg: ellocopo2::owned::Msg,
    usb_devices: USBDevices,
) -> Result<impl warp::Reply, Rejection> {
    let usb_devices = usb_devices.clone();
    //send_command(usb_devices).await;
    tokio::time::delay_for(std::time::Duration::from_secs(1)).await;
    let list = usb_devices.devices().await;
    let mut reply = warp::reply::json(&list);
    info!("List of devices: {:#?}", &list);
    if list.len() <= 0 {
        reply = warp::reply::json(&"No devices in list");
        return Ok(reply);
    }
    //assert!(list.len() > 0, "No devices in list");

    let dev = usb_devices.acquire_device(&list[0]["path"]).await;
    if msg.1 == "Wrong command" {
        reply = warp::reply::json(&msg.1);
        return Ok(reply);
    }
    if let Ok(Some((mut tx, mut rx))) = dev {
        tx.send(msg).await.expect("Echo: cant not send");
        match rx.next().await {
            Some(r) => {
                let ellocopo2::owned::Msg(code, path, value) = r;
                info!("Finally here!: {:?} {:?} {:?}", code, path, value);
                reply = warp::reply::json(&value);
            }
            None => error!("Echo: no recive data"),
        }
    } else {
        error!("Echo: no device channels");
        let T = "Echo: no device channels";
        reply = warp::reply::json(&T);
    }
    Ok(reply)
}

fn read_commands() -> Vec<String> {
    let contents = fs::read_to_string("/home/fmv/Документы/holter-bridge/scheme.json")
        .expect("Something went wrong reading the file");
    let mut commands: Vec<ellocopo2_codegen::parser::RegisterDesc> = Vec::new();
    let mut res: Vec<String> = Vec::new();
    commands = ellocopo2_codegen::parser::parser(&contents);
    for i in 0..commands.len() {
        let a = &commands[i].path;
        res.push(a.to_string());
    }
    println!("{:?}", res);
    res
}

pub fn create(
    addr: SocketAddr,
    state: GlobalState,
    usb_devices: USBDevices,
) -> impl std::future::Future {
    let usb_devices = warp::any().map(move || (usb_devices.clone()));

    let device_list = warp::path!("devices" / "list")
        .and(usb_devices.clone())
        .and_then(list_devices)
        .map(|reply| reply);

    let ws_vis = warp::path!("api" / "vis" / u32).map({
        let state = state.clone();
        move |i| {
            let flag = i != 0;
            state.vis.store(flag, Ordering::Relaxed);
            if flag {
                "On"
            } else {
                "Off"
            }
        }
    });

    let ws = warp::path!("socket").and(warp::ws()).map({
        let state = state.clone();
        move |ws: warp::ws::Ws| {
            let state = state.clone();
            ws.on_upgrade(|websocket| async move {
                let (mut tx, rx) = websocket.split();
                let (mut ch_tx, mut ch_rx) = mpsc::channel(100);

                tokio::spawn(async move {
                    while let Some(data) = ch_rx.next().await {
                        if let Err(e) = tx.send(warp::ws::Message::text(data)).await {
                            warn!("Failed, connection closed? {}", e);
                            break;
                        }
                    }
                });

                loop {
                    if state.vis.load(Ordering::Relaxed) {
                        ch_tx.send("Hello world!").await;
                    }
                    tokio::time::delay_for(std::time::Duration::from_millis(1000)).await;
                }
            })
        }
    });

    let commands: Vec<String> = read_commands();

    let commandone = warp::path("api")
        .and(warp::path("v1"))
        .and(warp::path::param::<String>())
        .map({
            let commands = commands.clone();
            move |name1: String| {
                let mut a: Vec<String> = Vec::new();
                a.push(name1);
                commands_parse(a, commands.clone())
            }
        })
        .and(usb_devices.clone())
        .and_then(send_command_web)
        .and(warp::path::end());

    let commandtwo = warp::path("api")
        .and(warp::path("v1"))
        .and(warp::path::param::<String>())
        .and(warp::path::param::<String>())
        .map({
            let commands = commands.clone();
            move |name1: String, name2: String| {
                let mut a: Vec<String> = Vec::new();
                a.push(name1);
                a.push(name2);
                commands_parse(a, commands.clone())
            }
        })
        .and(usb_devices.clone())
        .and_then(send_command_web)
        .and(warp::path::end());

    let commandthree = warp::path("api")
        .and(warp::path("v1"))
        .and(warp::path::param::<String>())
        .and(warp::path::param::<String>())
        .and(warp::path::param::<String>())
        .map({
            let commands = commands.clone();
            move |name1: String, name2: String, name3: String| {
                let mut a: Vec<String> = Vec::new();
                a.push(name1);
                a.push(name2);
                a.push(name3);
                commands_parse(a, commands.clone())
            }
        })
        .and(usb_devices.clone())
        .and_then(send_command_web)
        .and(warp::path::end());

    use ellocopo2::Value;
    use serde::{Deserialize, Serialize};

    let write = warp::post()
        .and(warp::body::content_length_limit(1024 * 16))
        .and(warp::body::json())
        .map(|mut content: String| {
            let val: Value = serde_json::from_str(&content).unwrap();
            warp::reply::json(&val)
        });

    let hello = warp::path("hello").map(|| "Hello");

    let commands = commandtwo.or(commandthree).or(commandone);
    let routes = warp::get().and(hello.or(ws_vis).or(ws).or(commands).or(device_list));

    warp::serve(routes).run(addr)
}
