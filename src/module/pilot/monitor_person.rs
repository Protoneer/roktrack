//! Monitoring Person Pilot

use std::sync::mpsc::Sender;

use super::PilotHandler;
use crate::module::{
    device::Roktrack,
    pilot::base,
    pilot::RoktrackState,
    util::{common::send_line_notify_with_image, init::RoktrackProperty},
    vision::detector::{Detection, FilterClass, RoktrackClasses},
    vision::VisionMgmtCommand,
};

pub struct MonitorPerson {
    last_detected_time: u64,
}

impl MonitorPerson {
    pub fn new() -> Self {
        Self {
            last_detected_time: 0,
        }
    }
}

impl Default for MonitorPerson {
    fn default() -> Self {
        Self::new()
    }
}

impl PilotHandler for MonitorPerson {
    /// Function called from a thread to handle the Monitor Person Pilot logic
    fn handle(
        &mut self,
        state: &mut RoktrackState,
        device: &mut Roktrack,
        detections: &mut [Detection],
        _tx: Sender<VisionMgmtCommand>,
        property: RoktrackProperty,
    ) {
        log::debug!("Start MonitorPerson Handle");
        // Assess and handle system safety
        let system_risk = match assess_system_risk(state, device) {
            Some(SystemRisk::StateOff) | Some(SystemRisk::HighTemp) => Some(base::stop(device)),
            None => None,
        };
        if system_risk.is_some() {
            log::debug!("System Risk Exists. Continue.");
            return; // Risk exists, continue
        }

        // Check prtson exist
        if !RoktrackClasses::filter(detections, RoktrackClasses::PERSON.to_u32()).is_empty() {
            log::warn!("Person Detected!!");
            device
                .inner
                .clone()
                .lock()
                .unwrap()
                .speak("person_detecting_warn");
            // Get now.
            let utc = chrono::Utc::now();
            if self.last_detected_time + 60000 < utc.timestamp_millis() as u64 {
                log::debug!("Interval time has elapsed. Re-detection is notified.");
                self.last_detected_time = utc.timestamp_millis() as u64;
                let _ = send_line_notify_with_image(
                    "Person detected.",
                    &property.path.img.last,
                    property.conf,
                );
            }
        }
        log::debug!("End MonitorPerson Handle");
    }
}

/// System Risks
///
#[derive(Debug, Clone)]
enum SystemRisk {
    StateOff,
    HighTemp,
}
/// Identify system-related risks
///
fn assess_system_risk(state: &RoktrackState, device: &Roktrack) -> Option<SystemRisk> {
    if !state.state {
        Some(SystemRisk::StateOff)
    } else if state.pi_temp > 70.0 {
        device.inner.clone().lock().unwrap().speak("high_temp");
        Some(SystemRisk::HighTemp)
    } else {
        None
    }
}
