pub use esp32_omni_track::prelude::*;

espidf_only! {
    pub fn main() -> Result<()> {
        // It is necessary to call this function once. Otherwise, some patches to the runtime
        // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
        esp_idf_svc::sys::link_patches();

        // Bind the log crate to the ESP Logging facilities
        esp_idf_svc::log::EspLogger::initialize_default();

        log::info!("Hello, world!");

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    // use super::*;

    #[test_log::test]
    fn test_it() {
        log::info!("Testing tests working...");
        assert_eq!(2 + 2, 4);
    }
}