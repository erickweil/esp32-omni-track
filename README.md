# esp32-omni-track
GPS tracking for ESP32 (Wifi+MQTT or LoRaWAN) written in Rust (std)

## Boards
For now the only boards that will be supported are: 
- `heltec_wireless_tracker` Heltec Wireless Tracker (Esp32S3FN8 Xtensa) with integrated GPS and LoRaWAN
- `esp32c3_super_mini` Esp32C3 Super Mini (esp32c3 RISC-V) + any UART GPS Module

## Roadmap

- [x] ./examples/primos.rs Rust com suporte std para o ESP32 (esp-idf-template)
- [x] ./examples/blink_http_server/ Utilização do Wifi
- [x] Utilizar Sistema de arquivos LittleFS
- [ ] Comunicação MQTT

- [x] ./examples/tft-st7735.rs Utilizar Display ST7735
- [x] ./examples/gps.rs Interação com módulo GPS via UART
- [ ] Comunicação LoRaWAN

## See also
- https://github.com/erickweil/arduino-projetos (Initial tracker system in c++ with PlatformIO and Rust code experiments for esp32)