#![allow(unused_imports)]
use std::{sync::{Arc, Mutex}, thread, time::Duration};
use mipidsi::{Builder, interface::SpiInterface, models::ST7735s, options::{ColorInversion, ColorOrder, Orientation, Rotation}};
use nmea::Nmea;

pub use esp32_omni_track::prelude::*;
mod gps;
use gps::*;

mod display;
use display::*;

mod wifi;
use wifi::*;

// Html das páginas, sem precisar de alocação dinâmica (String)
static INDEX_HTML: &str = include_str!("index.html");

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

    use esp_idf_svc::eventloop::EspSystemEventLoop;
    use esp_idf_svc::nvs::EspDefaultNvsPartition;
    use esp_idf_svc::wifi::{BlockingWifi, EspWifi};
    use esp_idf_svc::http;
    use esp_idf_svc::io::Write;

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
        let sys_loop = EspSystemEventLoop::take()?;
        let nvs = EspDefaultNvsPartition::take()?;

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

        #[cfg(feature = "heltec_wireless_tracker")]
        {
            // Habilita Vext para alimentar o display e módulo GNSS onboard
            // Se não fizer isso antes de tudo, corrompe a memória e dá todo tipo de bug
            vext.set_high()?;
            log::info!("Vext habilitado para alimentar o display e módulo GNSS onboard");
            // Espera um pouco para estabilizar tensão (Display, GPS, Wifi, etc...)
            thread::sleep(Duration::from_millis(500));
        }

        // Liga o backlight
        backlight.set_high()?;
        thread::sleep(Duration::from_millis(250));

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
        let mut buffer = [0u8; 512];
        let di = SpiInterface::new(
            spi_device, 
            dc, 
            &mut buffer
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

        let mut display_module = DisplayModule::new();
        let gps_module = GPSModule::new();

        log::info!("INICIALIZADO 1!");
        unsafe {
            log::info!("Stack mínima livre: {} bytes", sys::uxTaskGetStackHighWaterMark(core::ptr::null_mut()));
            log::info!("Free heap: {} bytes", sys::esp_get_free_heap_size());
            log::info!("Largest free block: {} bytes", sys::heap_caps_get_largest_free_block(sys::MALLOC_CAP_8BIT));
        }

        display_module.draw_position(&mut display, None)?;

        // Arc<Mutex<T>> permite compartilhar estado entre múltiplas rotas de forma segura
        let gps_module = Arc::new(Mutex::new(gps_module));

        // WIFI
        log::info!("Configurando Wi-Fi...");

        let mut wifi = BlockingWifi::wrap(
            EspWifi::new(peripherals.modem, sys_loop.clone(), Some(nvs))?,
            sys_loop,
        )?;
        let mut wifi_module = WifiModule::new();
        wifi_module.config_wifi(&mut wifi)?;

        // HTTP server
        let mut server = http::server::EspHttpServer::new(&http::server::Configuration {
            stack_size: 10240,
            ..Default::default()
        })?;
        server.fn_handler("/", http::Method::Get, move |req| {
            req.into_ok_response()?
                .write_all(INDEX_HTML.as_bytes())
                .map(|_| ())
        })?;

        let handler_gps_module = gps_module.clone();
        server.fn_handler("/position", http::Method::Get, move |req| {
            let position = handler_gps_module.lock().unwrap().get_last_fix();

            req.into_ok_response()?
                .write_all(
                    format!("{{ \"latitude\": \"{:}\", \"longitude\": \"{:}\", \"speed\": \"{:}\", \"course\": \"{:}\", \"hdop\": \"{:}\", \"num_satellites\": \"{:}\", \"timestamp\": \"{:}\" }}",
                    position.as_ref().and_then(|fix| fix.latitude).unwrap_or_default(),
                    position.as_ref().and_then(|fix| fix.longitude).unwrap_or_default(),
                    position.as_ref().and_then(|fix| fix.speed).unwrap_or_default(),
                    position.as_ref().and_then(|fix| fix.course).unwrap_or_default(),
                    position.as_ref().and_then(|fix| fix.hdop).unwrap_or_default(),
                    position.as_ref().and_then(|fix| fix.num_satellites).unwrap_or_default(),
                    position.as_ref().and_then(|fix| fix.timestamp).unwrap_or_default()
                    ).as_bytes()
                )
        })?;

        log::info!("INICIALIZADO 2!");
        unsafe {
            log::info!("Stack mínima livre: {} bytes", sys::uxTaskGetStackHighWaterMark(core::ptr::null_mut()));
            log::info!("Free heap: {} bytes", sys::esp_get_free_heap_size());
            log::info!("Largest free block: {} bytes", sys::heap_caps_get_largest_free_block(sys::MALLOC_CAP_8BIT));
        }

        let mut changed_before = false;
        loop {
            let changed = gps_module.lock().unwrap().read_from_uart(&mut gps_uart)?;

            if changed_before && !changed {
                let fix = gps_module.lock().unwrap().get_last_fix();
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
