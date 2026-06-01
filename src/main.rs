#![allow(unused_imports)]
use std::{thread, time::Duration};
use mipidsi::{Builder, interface::SpiInterface, models::ST7735s, options::{ColorInversion, ColorOrder, Orientation, Rotation}};
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
    use esp_idf_svc::hal::{
        gpio::{AnyIOPin, AnyInputPin, PinDriver},
        peripherals::Peripherals,
        spi::{self},
        uart::{UartConfig, UartDriver}, 
        units::{Hertz, MegaHertz},
    };

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
        log::info!("Configurando periféricos...");

        let peripherals = Peripherals::take()?;

        // Configuração de Pinos do display para o Heltec Wireless Tracker
        #[cfg(feature = "esp32c6_devkitc_1")]
        let spi = peripherals.spi2;
        #[cfg(feature = "esp32c6_devkitc_1")]
        let rst = PinDriver::output(peripherals.pins.gpio5)?;
        #[cfg(feature = "esp32c6_devkitc_1")]
        let dc = PinDriver::output(peripherals.pins.gpio4)?;
        #[cfg(feature = "esp32c6_devkitc_1")]
        let mut backlight = PinDriver::output(peripherals.pins.gpio24)?;
        #[cfg(feature = "esp32c6_devkitc_1")]
        let sclk = peripherals.pins.gpio6;
        #[cfg(feature = "esp32c6_devkitc_1")]
        let sda = peripherals.pins.gpio7;
        #[cfg(feature = "esp32c6_devkitc_1")]
        let cs = Some(peripherals.pins.gpio18);

        #[cfg(feature = "heltec_wireless_tracker")]
        let spi = peripherals.spi2;
        #[cfg(feature = "heltec_wireless_tracker")]
        let rst = PinDriver::output(peripherals.pins.gpio39)?;
        #[cfg(feature = "heltec_wireless_tracker")]
        let dc = PinDriver::output(peripherals.pins.gpio40)?;
        #[cfg(feature = "heltec_wireless_tracker")]
        let mut backlight = PinDriver::output(peripherals.pins.gpio21)?;
        #[cfg(feature = "heltec_wireless_tracker")]
        let sclk = peripherals.pins.gpio41;
        #[cfg(feature = "heltec_wireless_tracker")]
        let sda = peripherals.pins.gpio42;
        #[cfg(feature = "heltec_wireless_tracker")]
        let cs = Some(peripherals.pins.gpio38);

        let sdi = None::<AnyInputPin>;

        #[cfg(feature = "heltec_wireless_tracker")]
        let mut vext = PinDriver::output(peripherals.pins.gpio3)?; // Vext Ctrl: HIGH para energizar display e GNSS onboard

        log::info!("GPS UART...");
        // Default read from Serial port (UART0) for ESP32
        #[cfg(feature = "esp32c6_devkitc_1")]
        let gps_uart = UartDriver::new(
            peripherals.uart1,
            peripherals.pins.gpio11, // UART0 TX
            peripherals.pins.gpio10, // UART0 RX
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

        log::info!("ST7735 SPI...");
        // Inicializando Display ST7735s
        let spi_device = spi::SpiDeviceDriver::new_single(
            spi,
            sclk,
            sda,
            sdi,
            cs,
            &spi::SpiDriverConfig::new(),
            &spi::SpiConfig::new()
                .baudrate(MegaHertz(26).into()),
                //.data_mode(MODE_3), // note that in order for the ST7789 to work, the data_mode needs to be set to MODE_3
        )?;

        // display interface abstraction from SPI and DC
        // Buffer na heap para não pressionar a stack da task principal
        let mut buffer = Box::new([0u8; 512]);
        let di = SpiInterface::new(
            spi_device, 
            dc, 
            buffer.as_mut()
        );

        // crate driver
        let mut display = Builder::new(ST7735s, di)
            // Heltec Wireless Tracker
            .display_size(80, 160)
            .display_offset(26, 1)
            .color_order(ColorOrder::Bgr)
            .invert_colors(ColorInversion::Inverted)
            .orientation(Orientation::new().rotate(Rotation::Deg270))
            .reset_pin(rst)
            .init(&mut delay::Ets)
            .map_err(|e| format!("Erro ao inicializar display: {e:?}"))?;

        log::info!("Ativando...");

        #[cfg(feature = "heltec_wireless_tracker")]
        {                
            // Habilita Vext para alimentar o display e módulo GNSS onboard
            vext.set_high()?;
            //thread::sleep(Duration::from_millis(500));
            log::info!("Vext habilitado para alimentar o display e módulo GNSS onboard");
        }

        // Liga o backlight
        backlight.set_high()?;
        //thread::sleep(Duration::from_millis(50));

        let mut display_module = DisplayModule::new();
        let mut gps_module = GPSModule::new();

        log::info!("INICIALIZADO!");
        // uxTaskGetStackHighWaterMark retorna palavras (4 bytes), então multiplica por 4 para bytes.
        let high_water_words = unsafe { sys::uxTaskGetStackHighWaterMark(core::ptr::null_mut()) };
        log::info!("Stack mínima livre: {} bytes ({} words)", high_water_words * 4, high_water_words);

        let mut changed_before = false;
        loop {

            let changed = gps_module.read_from_uart(&mut gps_uart)?;

            if changed_before && !changed {
                let fix = gps_module.get_last_fix();
                if let Some(fix) = fix.as_ref() {
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

                display_module.draw_position(&mut display, fix.as_ref())?;
            }
            changed_before = changed;

            // Devolve o controle para o FreeRTOS
            thread::sleep(Duration::from_millis(10));
        }
    }
}
