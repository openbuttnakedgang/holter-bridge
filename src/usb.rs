
use futures::channel::mpsc;
use futures::channel::oneshot;
use futures::lock::Mutex;
use futures::prelude::*;
use libusb::{Context as CxUsb, DeviceHandle};
use crate::usbfutures::Device;
use std::collections::{hash_map::Entry, HashMap};
use std::sync::Arc;
use std::time::Duration;
use std::time::SystemTime;

///  USB Consts
pub const VID: u16 = 0x0483;
pub const PID: u16 = 0x7503;
pub const EP_VIS: u8 = 0x83;
pub const EP_OUT: u8 = 0x01;
pub const EP_IN: u8 = 0x81;
//const EP_OUT: u8 = 0x04;
//const EP_IN: u8 = 0x85;
pub const EP_DATA_OUT: u8 = 0x07;
pub const EP_DATA_IN: u8 = 0x86;

struct DeviceEntry {
    acquired: DeviceAcquiredState,
    product: String,
}

enum DeviceAcquiredState {
    Available,
    Acquired(mpsc::Sender<oneshot::Sender<()>>),
}

impl std::fmt::Debug for DeviceEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match &self.acquired {
            DeviceAcquiredState::Available => write!(f, "Avilable ({})", self.product)?,
            DeviceAcquiredState::Acquired { .. } => write!(f, "Acquired")?,
        }
        Ok(())
    }
}

impl DeviceEntry {
    pub fn new(product: &str) -> Self {
        DeviceEntry {
            acquired: DeviceAcquiredState::Available,
            product: product.to_string(),
        }
    }

    pub fn product(&self) -> &str {
        &self.product
    }

    pub fn acquire(&mut self, tx: mpsc::Sender<oneshot::Sender<()>>) {
        self.acquired = DeviceAcquiredState::Acquired(tx);
    }

    pub async fn release(&mut self) {
        match &mut self.acquired {
            DeviceAcquiredState::Acquired(tx) => {
                // We use a oneshot channel to communicate that the device has been successfully
                // dropped. The "device_loop" task will first drop the device and then drop this
                // Sender.
                let (close_tx, close_rx) = oneshot::channel();
                if let Err(_e) = tx.send(close_tx).await {
                    error!("failed to send");
                }
                let _ = close_rx.await; // Error here is expected
            }
            _ => (),
        }
        self.acquired = DeviceAcquiredState::Available;
    }
}

pub struct USBDevices {
    devices: Arc<Mutex<HashMap<String, DeviceEntry>>>,
    libusb: &'static CxUsb,
}

impl Clone for USBDevices {
    fn clone(&self) -> Self {
        USBDevices {
            devices: Arc::clone(&self.devices),
            libusb: self.libusb,
        }
    }
}


impl USBDevices {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        
        let cx = Box::new(CxUsb::new()?);
        let cx = Box::leak(cx);

        Ok(USBDevices {
            devices: Default::default(),
            libusb: cx,
        })
    }
    pub async fn devices(&self) -> Vec<HashMap<String, String>> {
        self.devices
            .lock()
            .await
            .iter()
            .map(|device| {
                let mut d = HashMap::new();
                d.insert(
                    "path".into(),
                    device.0.to_string(),
                );
                d.insert("product".into(), device.1.product().to_string());
                d
            })
            .collect()
    }

    pub async fn presence_detector(
        self,
        mut notify_rx: mpsc::Receiver<()>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        loop {
            // Wait here until we are notified of new request
            info!("Waiting notification request..");
            let _ = notify_rx.next().await;
            info!("Notified!");
            let mut last_seen = None;
            loop {
                self.refresh().await?;

                // Stop iterating in case wallets are plugged out and there haven't been any
                // communication in a while.
                if self.devices.lock().await.len() == 0 {
                    match last_seen {
                        None => last_seen = Some(SystemTime::now()),
                        Some(last_seen) => {
                            if last_seen.elapsed()? > Duration::from_secs(5) {
                                break;
                            }
                        }
                    }
                } else {
                    last_seen = None;
                }
                tokio::time::delay_for(Duration::from_millis(200)).await;
            }
        }
    }

    pub async fn refresh(&self) -> Result<(), Box<dyn std::error::Error>> {

        let libusb = self.libusb;

        let mut seen = Vec::new();
        let mut devices_guard = self.devices.lock().await;
        for device in libusb.devices().expect("No device list").iter() {
            
            let device_desc = device.device_descriptor()?;

            if device_desc.vendor_id() == VID && device_desc.product_id() ==  PID {
                let path = { 
                    device.bus_number().to_string() + ":" + &device.address().to_string()
                };
                //let product = match device.product_string.as_ref() {
                //    Some(product) => product,
                //    None => {
                //        warn!("ignored: no product");
                //        continue;
                //    }
                //};
                seen.push(path.clone());
                match devices_guard.entry(path.clone()) {
                    Entry::Occupied(_) => (),
                    Entry::Vacant(v) => {
                        info!("Found Holter monitor at {}!", path);
                        v.insert(DeviceEntry::new(&String::from("TODO:")));
                    }
                }
            }
        }
        // Remove all devices that wasn't seen
        devices_guard.retain(|k, _| seen.contains(&k));
        Ok(())
    }

    pub async fn acquire_device(
        &self,
        path: &str,
    ) -> Result<Option<(mpsc::Sender<Vec<u8>>, mpsc::Receiver<Vec<u8>>)>, Box<dyn std::error::Error>>
    {
        if let Some(device) = self.devices.lock().await.get_mut(path) {
            // Make sure device is released
            device.release().await;

            let (in_tx, in_rx) = mpsc::channel(128);
            let (out_tx, out_rx) = mpsc::channel(128);
            
            // TODO: use path
            let libusb_device = match self.libusb.open_device_with_vid_pid(VID, PID) {
                Some(device) => device,
                None => Err(libusb::Error::NoDevice)?
            };
            let libusb_device = Device::new(libusb_device)?;
            info!("Successfully acquired device: {}", path);
            let (on_close_tx, on_close_rx) = mpsc::channel(1);
            device.acquire(on_close_tx);
            tokio::spawn(device_loop(libusb_device, in_rx, out_tx, on_close_rx));
            Ok(Some((in_tx, out_rx)))
        } else {
            info!("Failed to acquire device: {}", path);
            Ok(None)
        }
    }
}

async fn handle_msg(
    device: &mut Device,
    msg: Vec<u8>,
    out_tx: &mut mpsc::Sender<Vec<u8>>,
) -> Result<(), Box<dyn std::error::Error>> {
    //let (cid, cmd, _) = u2fframing::parse_header(&msg[..])?;

    //let mut wscodec = U2FWS::with_cid(cid, cmd);
    //let res = wscodec.decode(&msg[..])?.ok_or(std::io::Error::new(
    //    std::io::ErrorKind::Other,
    //    "not enough data in websocket message",
    //))?;

    //let mut hidcodec = U2FHID::new(cmd);
    let mut buf = [0u8; 7 + 7609]; // Maximally supported size by u2f
    //let len = hidcodec.encode(&res[..], &mut buf[..])?;
    
    device.write_all(&msg[..]).await?;

    let mut len = 0;
    loop {
        let this_len = device.read(&mut buf[len..]).await?;
        len += this_len;

        if let Err(e) = out_tx.send(buf[..len].to_vec()).await {
            error!("Failed to send internally: {}", e);
        }
        break;

        ////let res = hidcodec.decode(&buf[..len])?;
        //if let Some(res) = res {
        //    if let Ok(len) = wscodec.encode(&res[..], &mut buf[..]) {
        //        if let Err(e) = out_tx.send(buf[..len].to_vec()).await {
        //            error!("Failed to send internally: {}", e);
        //        }
        //    }
        //    break;
        //}
        // Loop to read out more data from device
    }
    Ok(())
}

async fn device_loop(
    mut device: Device,
    mut in_rx: mpsc::Receiver<Vec<u8>>,
    mut out_tx: mpsc::Sender<Vec<u8>>,
    mut on_close_rx: mpsc::Receiver<oneshot::Sender<()>>,
) {
    loop {
        tokio::select! {
            msg = in_rx.next() => {
                if let Some(msg) = msg {
                    if let Err(e) = handle_msg(&mut device, msg, &mut out_tx).await {
                        error!("message ignored: {}", e);
                    }
                } else {
                    error!("dev channel closed");
                    return;
                }
            },
            close_tx = on_close_rx.next() => {
                if let Some(_close_tx) = close_tx {
                    // We drop the device explitly so that it is dropped before the Sender we were sent
                    drop(device);
                } else {
                    // When the device is plugged out, the other end of the channel will be dropped and
                    // then this future will resolve to None since the stream has ended.
                    info!("Device was plugged out");
                }
                return;
            }
        }
    }
}
