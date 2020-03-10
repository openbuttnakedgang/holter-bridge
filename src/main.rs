use futures::channel::mpsc;
use futures::prelude::*;
use std::net::SocketAddr;
use tokio::runtime::Runtime;
use ellocopo::MsgBuilder;
use ellocopo::OperationStatus;
use ellocopo::Error;

use crate::parser::pars_answer;

#[macro_use]
extern crate log;

//mod error;
mod usb;
//mod web;
mod usbfutures;
mod parser;

use usb::USBDevices;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Check if the user requested some specific log level via an env variable. Otherwise set log
    // level to something reasonable.
    if let Ok(_) = std::env::var("RUST_LOG") {
        env_logger::init();
    } else {
        env_logger::Builder::new()
            .filter_level(log::LevelFilter::Info)
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
        async move {
            tokio::time::delay_for(std::time::Duration::from_secs(1)).await;
            let list = usb_devices.devices().await;
            info!("List of devices: {:#?}", &list);
            assert!(list.len() > 0, "No devices in list");

            let dev = usb_devices.acquire_device(&list[0]["path"]).await;
            let mut echo = [0u8; 0x40];
            loop {
                if let Ok(Some((mut tx, mut rx))) = dev {
                    loop {
                        let request_sz = MsgBuilder::request(&mut echo)
                            .operation(OperationStatus::Read)
                            .name("build.profile")
                            .build();
                        let echo = &echo[0..request_sz];
                        let name = unsafe { core::str::from_utf8_unchecked(&echo[3..]) };
                        info!("builder name: {}", name);
                        info!("Echo In : {:x?}", &echo[..]);
                        tx.send(echo.to_vec()).await.expect("Echo: cant not send");
                        match rx.next().await {
                            Some(r) => {
                                info!("Echo Out : {:x?}", r);
                                //let r = &r[0..request_sz];
                                let ans = pars_answer(&r);
                                info!("Answer : {:x?}", ans);
                            }
                            None => error!("Echo: no recive data"),
                        };
                        tokio::time::delay_for(std::time::Duration::from_millis(250)).await; 
                    }
                }
                else { error!("Echo: no device channels") }
                tokio::time::delay_for(std::time::Duration::from_secs(1)).await;
            }
        }
    };

    let addr: SocketAddr = "127.0.0.1:3333".parse()?;

    println!("listening on http://{}", addr);
    //let server = web::create(usb_devices, notify_tx, addr);

    rt.block_on(async move {
        tokio::select! {
            //_ = server => info!("Warp returned"),
            _ = usb_poller => info!("Usb poller died"),
            _ = echo => info!("Echo ended"),
        }
    });

    Ok(())
}
