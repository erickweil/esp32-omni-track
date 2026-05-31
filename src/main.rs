#![allow(unused_imports)]
use std::{thread, time::Duration};
use nmea::Nmea;

pub use esp32_omni_track::prelude::*;
mod gps;
use gps::*;

mod display;
use display::*;

espidf_only! {
    use std::io::Read;
    use esp_idf_svc::sys as sys;
    use esp_idf_svc::io as embedded_io;
    use esp_idf_svc::{hal::delay};
    use esp_idf_svc::hal::{gpio::{AnyIOPin, PinDriver}, peripherals::Peripherals, uart::{UartConfig, UartDriver}, units::Hertz};

    /// Wrapper sobre UartDriver para implementar a trait `std::io::Read` de forma não bloqueante, permitindo ler os dados do GPS sem travar o loop principal.
    pub struct NonBlockingUart<'d>(pub UartDriver<'d>);

    impl std::io::Read for NonBlockingUart<'_> {
        fn read(&mut self, buf: &mut [u8]) -> core::result::Result<usize, std::io::Error> {
            match self.0.read(buf, delay::NON_BLOCK) {
                Ok(n) => Ok(n),
                Err(e) if e.code() == sys::ESP_ERR_TIMEOUT => Ok(0), // Sem dados disponíveis, retorna 0 bytes lidos
                Err(e) => Err(std::io::Error::new(std::io::ErrorKind::Other, format!("UART read error: {}", e))),
            }
        }
    }

    // A função main() será executada quando a placa ligar ou for resetada. Ela é o ponto de entrada do programa.
    pub fn main() -> Result<()> {
        // It is necessary to call this function once. Otherwise, some patches to the runtime
        // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
        esp_idf_svc::sys::link_patches();

        // Bind the log crate to the ESP Logging facilities
        esp_idf_svc::log::EspLogger::initialize_default();

        let peripherals = Peripherals::take()?;

        #[cfg(feature = "heltec_wireless_tracker")]
        let mut vext = PinDriver::output(peripherals.pins.gpio3)?; // Vext Ctrl: HIGH para energizar display e GNSS onboard

        #[cfg(feature = "heltec_wireless_tracker")]
        {                
            // Habilita Vext para alimentar o display e módulo GNSS onboard
            vext.set_high()?;
            thread::sleep(Duration::from_millis(500));
            log::info!("Vext habilitado para alimentar o display e módulo GNSS onboard");
        }

        // Default read from Serial port (UART0) for ESP32
        #[cfg(feature = "wokwi")]
        let gps_uart = UartDriver::new(
            peripherals.uart1,
            peripherals.pins.gpio17, // UART0 TX
            peripherals.pins.gpio16, // UART0 RX
            Option::<AnyIOPin>::None,
            Option::<AnyIOPin>::None,
            &UartConfig::new()
                .baudrate(Hertz(9600))
        )?;

        // Serial1.begin(115200, SERIAL_8N1, 33, 34);
        #[cfg(feature = "heltec_wireless_tracker")]
        let gps_uart = UartDriver::new(
            peripherals.uart1,
            peripherals.pins.gpio34, // UART1 TX
            peripherals.pins.gpio33, // UART1 RX
            Option::<AnyIOPin>::None,
            Option::<AnyIOPin>::None,
            &UartConfig::new()
                .baudrate(Hertz(115_200))
        )?;

        let mut gps_uart = NonBlockingUart(gps_uart);
        let mut gps_module = GPSModule::new();

        // Lê linha por linha do GPS
        log::info!("INICIALIZADO!");
        loop {
            let changed = gps_module.read_from_uart(&mut gps_uart)?;

            if changed {
                let fix = gps_module.get_last_fix();
                if let Some(fix) = fix {
                    log::info!("TIMESTAMP: {:?}", fix.timestamp);
                    log::info!("LATITUDE: {:?}", fix.latitude);
                    log::info!("LONGITUDE: {:?}", fix.longitude);
                    log::info!("SPEED: {:?}", fix.speed);
                    log::info!("COURSE: {:?}", fix.course);
                    log::info!("HDOP: {:?}", fix.hdop);
                    log::info!("NUM SATELLITES: {:?}", fix.num_satellites);
                } else {
                    log::info!("Sem fix GPS válido");
                }
            }

            // Devolve o controle para o FreeRTOS
            thread::sleep(Duration::from_millis(10));
        }
    }
}
