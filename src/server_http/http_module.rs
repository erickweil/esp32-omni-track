use std::{sync::{Arc, Mutex}, thread, time::Duration};

pub use esp32_omni_track::prelude::*;
use crate::gps::GPSPosition;

// Html das páginas, sem precisar de alocação dinâmica (String)
static INDEX_HTML: &str = include_str!("index.html");

pub struct HTTPAppData {
    pub last_gps_fix: Option<GPSPosition>,
}

#[cfg(feature = "espidf")]
mod wifi_module_impl {
    use super::*;

    use esp_idf_svc::wifi::{BlockingWifi, EspWifi};
    use esp_idf_svc::http;
    use esp_idf_svc::io::Write;

    pub fn setup_http_server(app_state: Arc<Mutex<HTTPAppData>>) -> Result<http::server::EspHttpServer<'static>> {
        let mut server = http::server::EspHttpServer::new(&http::server::Configuration {
            ..Default::default()
        })?;

        server.fn_handler("/", http::Method::Get, move |req| {
            req.into_ok_response()?
                .write_all(INDEX_HTML.as_bytes())
                .map(|_| ())
        })?;

        let handler_state = app_state.clone();
        server.fn_handler("/position", http::Method::Get, move |req| {
            let position = {
                let state = handler_state.lock().unwrap();
                state.last_gps_fix.clone()
            };

            let mut response = req.into_ok_response()?;
            write!(response, "{{ \"latitude\": \"{:}\", \"longitude\": \"{:}\", \"speed\": \"{:}\", \"course\": \"{:}\", \"hdop\": \"{:}\", \"num_satellites\": \"{:}\", \"timestamp\": \"{:}Z\" }}",
                position.as_ref().and_then(|fix| fix.latitude).unwrap_or_default(),
                position.as_ref().and_then(|fix| fix.longitude).unwrap_or_default(),
                position.as_ref().and_then(|fix| fix.speed).unwrap_or_default(),
                position.as_ref().and_then(|fix| fix.course).unwrap_or_default(),
                position.as_ref().and_then(|fix| fix.hdop).unwrap_or_default(),
                position.as_ref().and_then(|fix| fix.num_satellites).unwrap_or_default(),
                position.as_ref().and_then(|fix| fix.timestamp).unwrap_or_default(), // Z para indicar UTC
            )?;

            Result::Ok(())
        })?;

        Ok(server)
    }
}

#[cfg(feature = "espidf")]
pub use wifi_module_impl::*;