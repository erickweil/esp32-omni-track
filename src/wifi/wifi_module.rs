use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use esp32_omni_track::prelude::*;

/// Configurações de Wi-Fi
const SSID: &str = match option_env!("WIFI_SSID") {
    Some(ssid) => ssid,
    None => "Wokwi-GUEST",
};
const PASSWORD: &str = match option_env!("WIFI_PASS") {
    Some(pass) => pass,
    None => "",
};

// true para Access Point, false para Client
const WIFI_AP_MODE: bool = match option_env!("WIFI_AP") {
    Some(val) => val.eq_ignore_ascii_case("true"),
    None => false,
};
const CHANNEL: u8 = 2;
/// O intervalo permitido é de [8, 84], o que corresponde a uma potência real de 2 dBm a 20 dBm.
/// A unidade do parâmetro de potência (power) é de 0.25 dBm.
/// 34 * 0.25 dBm = 8.5 dBm
/// [No esp32c3 precisa disso ou não funciona o wifi]
const MAX_RADIO_POWER: Option<i8> = None; // Some(34);

const WIFI_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

#[cfg(feature = "espidf")]
mod wifi_module_impl {
    use super::*;

    use esp_idf_svc::{
        http::{Method, server::EspHttpServer},
        io::Write,
        sys::esp_wifi_set_max_tx_power,
        wifi::{AccessPointConfiguration, AuthMethod, ClientConfiguration, Configuration},
    };

    use esp_idf_svc::eventloop::EspSystemEventLoop;
    use esp_idf_svc::hal::{gpio, peripherals::Peripherals};
    use esp_idf_svc::nvs::EspDefaultNvsPartition;
    use esp_idf_svc::wifi::{BlockingWifi, EspWifi};

    pub struct WifiModule {
        wifi: BlockingWifi<EspWifi<'static>>,
        last_connect_try: Option<Instant>,
    }

    impl WifiModule {
        pub fn new(wifi: BlockingWifi<EspWifi<'static>>) -> Self {
            WifiModule { 
                wifi,
                last_connect_try: None,
            }
        }

        pub fn config_wifi(&mut self) -> Result<()> {
            let auth_method = if PASSWORD.is_empty() {
                AuthMethod::None
            } else {
                AuthMethod::WPAWPA2Personal
            };

            let wifi_configuration = if WIFI_AP_MODE {
                Configuration::AccessPoint(AccessPointConfiguration {
                    ssid: SSID.try_into().unwrap(),
                    ssid_hidden: false,
                    auth_method,
                    password: PASSWORD.try_into().unwrap(),
                    channel: CHANNEL,
                    ..Default::default()
                })
            } else {
                Configuration::Client(ClientConfiguration {
                    ssid: SSID.try_into().unwrap(),
                    bssid: None,
                    auth_method,
                    password: PASSWORD.try_into().unwrap(),
                    channel: None,
                    pmf_cfg: esp_idf_svc::wifi::PmfConfiguration::Capable { required: false },
                    ..Default::default()
                })
            };

            self.wifi.set_configuration(&wifi_configuration)?;

            log::info!("Starting Wi-Fi...");
            self.wifi.start()?;

            // --- Redução da Potência de Transmissão (Hardware Brownout Fix) ---
            // O problema: o rádio RF liga e puxa um pico de corrente repentino que pode passar de 300mA.
            // Reduzir a potência de transmissão (ironicamente) ajuda a estabilizar o sinal nessas placas, pois diminui o ruído no circuito regulador de tensão interno.
            // SAFETY: é só um binding do código C
            if let Some(power) = MAX_RADIO_POWER {
                unsafe {
                    let err = esp_wifi_set_max_tx_power(power);
                    if err != 0 {
                        log::warn!("Aviso: Falha ao ajustar TX power. Codigo do erro: {}", err);
                    }
                }
            }

            if WIFI_AP_MODE {
                log::info!("Created Wi-Fi AP with WIFI_SSID `{SSID}` and password `{PASSWORD}`");
            } else {
                log::info!("Configured Wi-Fi client with WIFI_SSID `{SSID}` and password `{PASSWORD}`");
            }

            Ok(())
        }

        /// Método não-bloqueante para verificar e manter a conexão Wi-Fi (Caso perca conexão tenta reconectar continuamente a cada 30s)
        pub fn check_connect_wifi(&mut self) -> Result<bool> {
            let wifi = self.wifi.wifi_mut();

            if !WIFI_AP_MODE {
                // Só é necessário conectar se for modo cliente
                // self.wifi.connect()?;
                let is_connected = wifi.is_connected()?;
                if !is_connected {
                    if self.last_connect_try.is_none_or(|time| time.elapsed() > WIFI_CONNECT_TIMEOUT) {
                        log::info!("(re)conectando ao Wi-Fi...");
                        wifi.connect()?;
                        self.last_connect_try = Some(std::time::Instant::now());
                    }
                    return Ok(false);
                }
            }

            // self.wifi.wait_netif_up()?;
            let is_up = wifi.is_up()?;
            return Ok(is_up);
        }

        pub fn get_ip(&self) -> Option<std::net::IpAddr> {
            if WIFI_AP_MODE {
                self.wifi.wifi().ap_netif().get_ip_info().ok().map(|ip_info| ip_info.ip.into())
            } else {
                self.wifi.wifi().sta_netif().get_ip_info().ok().map(|ip_info| ip_info.ip.into())
            }
        }
    }
}

#[cfg(feature = "espidf")]
pub use wifi_module_impl::*;
