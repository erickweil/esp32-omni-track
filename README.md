# esp32-omni-track
GPS tracking for ESP32 (Wifi+MQTT or LoRaWAN) written in Rust (esp-idf std)

## Boards
For now the only boards that will be supported are: 
- `heltec_wireless_tracker` Heltec Wireless Tracker (Esp32S3FN8 Xtensa) with integrated GPS and LoRaWAN
- `esp32c3_super_mini` Esp32C3 Super Mini (esp32c3 RISC-V) + any UART GPS Module

## Roadmap
- [x] Coletar informações de posição do módulo GPS via UART + exibição display ST7735
- [ ] Modo stand alone, Wifi AP + Servidor web local
- [ ] Sistema de arquivos LittleFS para armazenar posições em fila
- [ ] Registro de posições por tempo e ângulo
- [ ] Modo mqtt wifi, comunicação com Wifi + MQTT para envio de posições
- [ ] Segurança, Túnel TLS na comunicação MQTT
- [ ] Modo LoRaWAN Tipo C, Comunicação com LoRaWAN para envio de posições
- [ ] Economia de energia, detecção parado vs movimento diferentes intervalos de envio, deep sleep
- [ ] Suporte a mais placas e módulos GPS
- [ ] Modo mqtt gprs/e/3g/4g/lte, comunicação com dados móveis + MQTT para envio de posições

## Desafios/Arquitetura/Detalhes de implementação
- [ ] BSP (Board Support Package) abstração para configuração de cada placa.
- [ ] Decidir como lidar com várias tarefas (ex: leitura GPS enquanto processa wifi e/ou LoRaWAN):
1. Multithreading (std threads são tasks do RTOS? problema da stack size)
2. Async (Embassy é compatível com esp-idf + std?)
3. Single thread + state machine (evitar operações bloqueantes quando possível, no projeto antigo era assim)

## See also
- https://github.com/erickweil/arduino-projetos (Initial tracker system in c++ with PlatformIO and Rust code experiments for esp32)