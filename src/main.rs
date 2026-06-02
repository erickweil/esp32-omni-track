#![allow(unused_imports)]
use std::{sync::{Arc, Mutex}, thread, time::Duration};
use embedded_graphics::draw_target::DrawTarget;
use mipidsi::{Builder, interface::SpiInterface, models::ST7735s, options::{ColorInversion, ColorOrder, Orientation, Rotation}};
use nmea::Nmea;

pub use esp32_omni_track::prelude::*;
mod gps;
use gps::*;

mod display;
use display::*;

mod wifi;
use wifi::*;

mod server_http;
use server_http::*;

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

    fn test_debug_memory() {
        unsafe {
            log::info!("Stack mínima livre: {} bytes", sys::uxTaskGetStackHighWaterMark(core::ptr::null_mut()));
            log::info!("Free heap: {} bytes", sys::esp_get_free_heap_size());
            log::info!("Largest free block: {} bytes", sys::heap_caps_get_largest_free_block(sys::MALLOC_CAP_8BIT));
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
        
        // =========================
        //   Pinagem de cada placa
        // =========================

        #[cfg(feature = "esp32c6_devkitc_1")]
        let (
            display_spi, display_rst, display_dc, mut display_backlight, display_sclk, display_sda, display_cs, display_sdi, display_spi_config,
            gps_uart, gps_rx, gps_tx, gps_uart_config
        ) = (
            peripherals.spi2, // spi
            PinDriver::output(peripherals.pins.gpio5)?, // rst
            PinDriver::output(peripherals.pins.gpio4)?, // dc
            PinDriver::output(peripherals.pins.gpio24)?, // backlight
            peripherals.pins.gpio6, // sclk
            peripherals.pins.gpio7, // sda
            Some(peripherals.pins.gpio18), // cs
            None::<AnyInputPin>, // sdi
            &spi::SpiConfig::new().baudrate(MegaHertz(26).into()), // Display SPI Config

            peripherals.uart1, // GPS UART
            peripherals.pins.gpio10, // UART1 RX
            peripherals.pins.gpio11, // UART1 TX
            &UartConfig::new().baudrate(Hertz(9600)) // GPS UART Config
        );
        #[cfg(feature = "heltec_wireless_tracker")]
        let (
            display_spi, display_rst, display_dc, mut display_backlight, display_sclk, display_sda, display_cs, display_sdi, display_spi_config,
            gps_uart, gps_rx, gps_tx, gps_uart_config, 
            mut vext
        ) = (
            peripherals.spi2, // spi
            PinDriver::output(peripherals.pins.gpio39)?, // rst
            PinDriver::output(peripherals.pins.gpio40)?, // dc
            PinDriver::output(peripherals.pins.gpio21)?, // backlight
            peripherals.pins.gpio41, // sclk
            peripherals.pins.gpio42, // sda
            Some(peripherals.pins.gpio38), // cs
            None::<AnyInputPin>, // sdi
            &spi::SpiConfig::new().baudrate(MegaHertz(26).into()), // Display SPI Config

            peripherals.uart1, // GPS UART
            peripherals.pins.gpio33, // UART1 RX
            peripherals.pins.gpio34, // UART1 TX
            &UartConfig::new().baudrate(Hertz(115_200)), // GPS UART Config

            PinDriver::output(peripherals.pins.gpio3)?, // Vext Ctrl: HIGH para energizar display e GNSS onboard
        );

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
        display_backlight.set_high()?;
        thread::sleep(Duration::from_millis(250));


        // =========================
        //   Configuração drivers
        // =========================

        log::info!("GPS UART...");
        // Serial1.begin(115200, SERIAL_8N1, 33, 34);
        let gps_uart_driver = UartDriver::new(
            gps_uart,
            gps_tx, // UART1 TX
            gps_rx, // UART1 RX
            Option::<AnyIOPin>::None,
            Option::<AnyIOPin>::None,
            gps_uart_config
        )?;
        let gps_uart_driver = NonBlockingUart(gps_uart_driver);
        let mut gps_module = GPSModule::new(gps_uart_driver);

        log::info!("ST7735 SPI...");
        // Inicializando Display ST7735s
        let display_spi_device = spi::SpiDeviceDriver::new_single(
            display_spi,
            display_sclk,
            display_sda,
            display_sdi,
            display_cs,
            &spi::SpiDriverConfig::new(),
            display_spi_config,
        )?;

        // display interface abstraction from SPI and DC
        let mut display_spi_interface_buffer = [0u8; 512];
        let display_spi_interface = SpiInterface::new(
            display_spi_device, 
            display_dc, 
            &mut display_spi_interface_buffer
        );

        // crate driver
        let display_driver = Builder::new(ST7735s, display_spi_interface)
            // Heltec Wireless Tracker
            .display_size(80, 160)
            .display_offset(26, 1)
            .color_order(ColorOrder::Bgr)
            .invert_colors(ColorInversion::Inverted)
            .orientation(Orientation::new().rotate(Rotation::Deg270))
            .reset_pin(display_rst)
            .init(&mut delay::Ets)
            .map_err(|e| format!("Erro ao inicializar display: {e:?}"))?;
        let mut display_module = DisplayModule::new(display_driver);

        log::info!("INICIALIZADO 1!");
        test_debug_memory();

        display_module.draw_position(None, None)
            .map_err(|e| format!("Erro no display: {e:?}"))?;

        // WIFI
        log::info!("Configurando Wi-Fi...");

        let wifi = BlockingWifi::wrap(
            EspWifi::new(peripherals.modem, sys_loop.clone(), Some(nvs))?,
            sys_loop,
        )?;
        let mut wifi_module = WifiModule::new(wifi);
        wifi_module.config_wifi()?;

        // HTTP server
        // Arc<Mutex<T>> permite compartilhar estado entre múltiplas rotas de forma segura
        let http_app_data = Arc::new(Mutex::new(HTTPAppData { 
            last_gps_fix: None,
        }));
        let _server = setup_http_server(http_app_data.clone())?;

        log::info!("INICIALIZADO 2!");
        test_debug_memory();

        let mut changed_before = false;
        let mut wifi_connect_timeout: Option<std::time::Instant> = None;
        loop {
            // Verifica conexão wifi
            if !wifi_module.is_connected() 
            && wifi_connect_timeout.is_none_or(|timeout| timeout.elapsed() > Duration::from_secs(60)) {
                log::info!("Tentando conexão Wi-Fi...");
                if let Err(e) = wifi_module.wait_connect_wifi() {
                    log::error!("Erro ao conectar Wi-Fi: {e}");
                    wifi_connect_timeout = Some(std::time::Instant::now());
                }
            }

            let changed = gps_module.read_from_uart()?;

            if changed_before && !changed {
                let fix = gps_module.get_last_fix();
                {
                    let mut app_state = http_app_data.lock().unwrap();
                    app_state.last_gps_fix = fix.clone();
                }

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

                display_module.draw_position(fix.as_ref(), wifi_module.get_ip())
                    .map_err(|e| format!("Erro no display: {e:?}"))?;
            }
            changed_before = changed;

            // Devolve o controle para o FreeRTOS
            thread::sleep(Duration::from_millis(10));
        }
    }
}
