//! BLE Receiver Module
//!
//! This module provides functionality to handle BLE (Bluetooth Low Energy) communications.

use crate::module::pilot::Modes;
use bitreader::BitReader;
use btleplug::api::{bleuuid::BleUuid, Central, CentralEvent, Manager as _, ScanFilter};
use btleplug::platform::{Adapter, Manager};
use futures::stream::StreamExt;
use std::process::Command;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;

/// BLE Broadcast Handler
pub struct BleBroadCast {
    pub inner: Arc<Mutex<BleBroadCastInner>>,
}

/// Default implementation for BLE Broadcast Handler
impl Default for BleBroadCast {
    fn default() -> Self {
        Self::new()
    }
}

impl BleBroadCast {
    /// Creates a new instance of BLE Broadcast Handler
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(BleBroadCastInner::new())),
        }
    }

    /// Listens to BLE advertisements and sends neighbor information via a channel.
    ///
    /// /// https://github.com/deviceplug/btleplug/blob/master/examples/discover_adapters_peripherals.rs
    pub fn listen(&self, tx: Sender<Neighbor>) -> JoinHandle<()> {
        thread::spawn(move || {
            log::debug!("Com Thread Started");
            // Create an asynchronous runtime.
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            // Run asynchronous tasks at runtime.
            rt.block_on(async {
                let manager = Manager::new().await.unwrap();

                // Get the first Bluetooth adapter.
                let central = Self::get_central(&manager).await;

                // Create an event stream for the adapter.
                let mut events = central.events().await.unwrap();

                // Start scanning for devices.
                central.start_scan(ScanFilter::default()).await.unwrap();

                while let Some(event) = events.next().await {
                    match event {
                        CentralEvent::DeviceDiscovered(id) => {
                            format!("DeviceDiscovered: {:?}", id);
                        }
                        CentralEvent::DeviceConnected(id) => {
                            format!("DeviceConnected: {:?}", id);
                        }
                        CentralEvent::DeviceDisconnected(id) => {
                            format!("DeviceDisconnected: {:?}", id);
                        }
                        CentralEvent::ManufacturerDataAdvertisement {
                            id,
                            manufacturer_data,
                        } => {
                            log::debug!(
                                "id:{}, key:{:?}, data:{:?}",
                                id.clone().to_string(),
                                *manufacturer_data.keys().last().unwrap(),
                                manufacturer_data.values().last().unwrap()
                            );
                            let manufacturer_id: u16 = *manufacturer_data.keys().last().unwrap();
                            let data: &Vec<u8> = manufacturer_data.values().last().unwrap();
                            if manufacturer_id == 65535 {
                                // Get the MAC address.
                                let mut mac_addr: String = id.to_string();
                                mac_addr = mac_addr.replace("hci0/dev_", "");
                                mac_addr = mac_addr.replace('_', ":");

                                // Generate neighbor information.
                                let mut neighbor = Neighbor::from_manufacture_data(data);
                                neighbor.mac = mac_addr.clone();
                                neighbor.manufacturer_id = manufacturer_id;
                                tx.send(neighbor).unwrap();
                                log::debug!(
                                    "BLE BroadCast Received From: {:?}, Content: {:?}",
                                    mac_addr,
                                    data
                                );
                            }
                        }
                        CentralEvent::ServiceDataAdvertisement { id, service_data } => {
                            format!("ServiceDataAdvertisement: {:?}, {:?}", id, service_data);
                        }
                        CentralEvent::ServicesAdvertisement { id, services } => {
                            let services: Vec<String> =
                                services.into_iter().map(|s| s.to_short_string()).collect();
                            format!("ServicesAdvertisement: {:?}, {:?}", id, services);
                        }
                        _ => {}
                    }
                }
            });
        })
    }

    /// Get the first available Bluetooth adapter.
    async fn get_central(manager: &Manager) -> Adapter {
        let adapters = manager.adapters().await.unwrap();
        adapters.into_iter().next().unwrap()
    }
}

/// BLE Broadcast Handler Inner
#[derive(Default)]
pub struct BleBroadCastInner {}

impl BleBroadCastInner {
    /// Creates a new instance of the BLE Broadcast Handler Inner.
    pub fn new() -> Self {
        // Set Advertisement Interval using hcitool commands.
        let _output = Command::new("hcitool")
            .args([
                "-i", "hci0", "cmd", "0x08", "0x0006", "A0", "00", "A0", "00", "03", "00", "00",
                "00", "00", "00", "00", "00", "00", "07", "00",
            ])
            .output()
            .expect("failed");

        // Start Advertisement using hcitool commands.
        let _output = Command::new("hcitool")
            .args(["-i", "hci0", "cmd", "0x08", "0x000a", "01"])
            .output()
            .expect("failed");

        Self {}
    }

    /// Broadcasts the advertisement data.
    pub fn cast(&self, identifier: &u8, data: Vec<u8>) {
        // Payload identifier and data in hexadecimal format.
        let payload_identifier = format!("{:02X}", identifier);
        let payload_data: Vec<_> = data.iter().map(|x| format!("{:02X}", x)).collect();

        // Combine payload elements.
        let mut payload: Vec<String> = vec![payload_identifier];
        payload.extend(payload_data);

        // Header and content for advertisement.
        let header: Vec<&str> = vec![
            "-i", "hci0", "cmd", "0x08", "0x0008", "1E", "02", "01", "06", "1A", "FF", "FF", "FF",
        ];
        let header: Vec<String> = header.iter().map(|x| x.to_string()).collect();
        let mut content: Vec<String> = vec![];
        content.extend(header);
        content.extend(payload);

        // Execute hcitool command for advertisement.
        let _output = Command::new("hcitool")
            .args(content)
            .output()
            .expect("failed");
    }
}

/// Neighbor State
#[derive(Debug, Clone)]
pub struct Neighbor {
    pub timestamp: String,
    pub rssi: i8,
    pub mac: String,
    pub manufacturer_id: u16,
    pub identifier: u8,
    pub state: bool,
    pub rest: u8,
    pub pi_temp: u8,
    pub mode: Modes,
    pub msg: u8,
    pub dest: u8,
}

impl Neighbor {
    /// Generates neighbor state from advertisement data.
    pub fn from_manufacture_data(data: &[u8]) -> Self {
        // Parse data elements.
        // Since the first 3 bytes of the data acquired by btleplug are filled with FF,
        // the data should be acquired from the 4th byte.
        let identifier = data[3];
        let buf = [data[4]];
        let mut bit_reader = BitReader::new(&buf);
        let state: bool = bit_reader.read_u8(1).unwrap() != 0;
        let rest: u8 = bit_reader.read_u8(7).unwrap();
        let pi_temp = data[5];
        let mode = data[6];
        let msg = data[7];
        let dest = data[8];

        // Set neighbor information.
        Self {
            timestamp: chrono::Utc::now().timestamp().to_string(),
            rssi: 0,
            mac: String::from(""),
            manufacturer_id: 0,
            identifier,
            state,
            rest,
            pi_temp,
            mode: Modes::from_u8(mode),
            msg,
            dest,
        }
    }
}

/// Child Message
#[derive(PartialEq)]
pub enum ChildMsg {
    Halt,
    Bumped,
    PersonFoundPause,
    ReachTarget,
    TargetLost,
    NewTargetFound,
    FromCwToCcw,
    PiTempHighHalt,
    MissionComplete,
    TargetNotFound,
    LeaderWaiting,
    TarailerPrepaired,
    ClimbUp,
    ClimbDown,
    Ack,
    PersonFoundWarn,
    AnimalFound,
    Unknown,
}

impl ChildMsg {
    /// Converts a u8 value to a ChildMsg enum.
    #[allow(dead_code)]
    pub fn from_u8(i: u8) -> ChildMsg {
        match i {
            0 => ChildMsg::Halt,
            1 => ChildMsg::Bumped,
            2 => ChildMsg::PersonFoundPause,
            3 => ChildMsg::ReachTarget,
            4 => ChildMsg::TargetLost,
            5 => ChildMsg::NewTargetFound,
            6 => ChildMsg::FromCwToCcw,
            7 => ChildMsg::PiTempHighHalt,
            8 => ChildMsg::MissionComplete,
            9 => ChildMsg::TargetNotFound,
            10 => ChildMsg::LeaderWaiting,
            11 => ChildMsg::TarailerPrepaired,
            12 => ChildMsg::ClimbUp,
            13 => ChildMsg::ClimbDown,
            14 => ChildMsg::Ack,
            15 => ChildMsg::PersonFoundWarn,
            16 => ChildMsg::AnimalFound,
            _ => ChildMsg::Unknown,
        }
    }

    /// Converts a ChildMsg enum to a u8 value.
    #[allow(dead_code)]
    pub fn to_u8(msg: ChildMsg) -> u8 {
        match msg {
            ChildMsg::Halt => 0,
            ChildMsg::Bumped => 1,
            ChildMsg::PersonFoundPause => 2,
            ChildMsg::ReachTarget => 3,
            ChildMsg::TargetLost => 4,
            ChildMsg::NewTargetFound => 5,
            ChildMsg::FromCwToCcw => 6,
            ChildMsg::PiTempHighHalt => 7,
            ChildMsg::MissionComplete => 8,
            ChildMsg::TargetNotFound => 9,
            ChildMsg::LeaderWaiting => 10,
            ChildMsg::TarailerPrepaired => 11,
            ChildMsg::ClimbUp => 12,
            ChildMsg::ClimbDown => 13,
            ChildMsg::Ack => 14,
            ChildMsg::PersonFoundWarn => 15,
            ChildMsg::AnimalFound => 16,
            _ => 255,
        }
    }
}

/// Parent Message
#[derive(PartialEq)]
pub enum ParentMsg {
    Off,
    On,
    Reset,
    Stop,
    Forward,
    Backward,
    Left,
    Right,
    Fill,
    Oneway,
    Climb,
    Around,
    MonitorPerson,
    MonitorAnimal,
    RoundTrip,
    FollowPerson,
    Unknown,
}

impl ParentMsg {
    /// Converts a u8 value to a ParentMsg enum.
    #[allow(dead_code)]
    pub fn from_u8(i: u8) -> ParentMsg {
        match i {
            0 => ParentMsg::Off,
            1 => ParentMsg::On,
            2 => ParentMsg::Reset,
            3 => ParentMsg::Stop,
            4 => ParentMsg::Forward,
            5 => ParentMsg::Backward,
            6 => ParentMsg::Left,
            7 => ParentMsg::Right,
            10 => ParentMsg::Fill,
            11 => ParentMsg::Oneway,
            12 => ParentMsg::Climb,
            13 => ParentMsg::Around,
            14 => ParentMsg::MonitorPerson,
            15 => ParentMsg::MonitorAnimal,
            16 => ParentMsg::RoundTrip,
            17 => ParentMsg::FollowPerson,
            _ => ParentMsg::Unknown,
        }
    }
}
