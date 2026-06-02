BOARD	   	    ?= heltec_wireless_tracker
# BOARD	   	    ?= esp32c3_super_mini
# BOARD	   	    ?= esp32c6_devkitc_1

# Mapeamento BOARD → MCU
ifneq ($(filter $(BOARD),heltec_wireless_tracker),)
  MCU             := esp32s3
  FLASH_SIZE      := 8mb
  PARTITION_TABLE := partitions/large_spiffs_v2_8MB.csv
else ifneq ($(filter $(BOARD),esp32c3_super_mini),)
  MCU             := esp32c3
  FLASH_SIZE      := 4mb
  PARTITION_TABLE := partitions/partitions_singleapp.csv
else ifneq ($(filter $(BOARD),esp32c6_devkitc_1),)
  MCU             := esp32c6
  FLASH_SIZE      := 8mb
  PARTITION_TABLE := partitions/large_spiffs_v2_8MB.csv
else
  $(error "Unknown BOARD: $(BOARD)")
endif

# Mapeamento MCU → TARGET
ifneq ($(filter $(MCU),esp32c3),)
  TARGET := riscv32imc-esp-espidf
else ifneq ($(filter $(MCU),esp32c6),)
  TARGET := riscv32imac-esp-espidf
else ifeq ($(MCU), esp32s3)
  TARGET := xtensa-esp32s3-espidf
else
  $(error "Unknown MCU: $(MCU)")
endif

BINARY     := esp32-omni-track
BUILD_TYPE := release
ELF        := target/$(TARGET)/$(BUILD_TYPE)/$(BINARY)
PORT       ?= # ex: PORT=/dev/ttyUSB0

build-rust:
# Para placas Xtensa tem que ser com esse aqui
# Precisa ter instalado o espup "espup install"
ifeq ($(TARGET), xtensa-esp32s3-espidf)
	. ${HOME}/export-esp.sh && \
	export MCU=$(MCU) && \
	cargo +esp build --target $(TARGET) --no-default-features --features "$(BOARD)" --release
else ifneq ($(filter $(TARGET),riscv32imc-esp-espidf riscv32imac-esp-espidf),)
# Placas RISC-V (ex: ESP32-C3) pode ser rust normal
# Precisa ter instalado a toolchain com rust-src e o target "rustup target add riscv32imc-unknown-none-elf" para ESP32-C3
	export MCU=$(MCU) && \
	cargo +nightly build --target ${TARGET} --no-default-features --features "$(BOARD)" --release
else
	$(error "Unknown TARGET: $(TARGET)")
endif

## Faz flash na placa (usa espflash com monitor integrado)
## https://github.com/esp-rs/esp-idf-template#flash
## https://github.com/esp-rs/esp-idf-sys/issues/385 (Detalhes sobre partitions.csv)
flash: build-rust
	espflash flash $(ELF) --partition-table $(PARTITION_TABLE) --monitor $(if $(PORT),--port $(PORT),)

## Apenas monitora a porta serial (sem flash)
monitor:
	espflash monitor $(if $(PORT),--port $(PORT),)

## Executa os testes (NO HOST)
test:
	export MCU=$(MCU) && \
	export RUST_BACKTRACE=1 && \
	cargo test --no-default-features --target "x86_64-unknown-linux-gnu" -- --nocapture

## Simula no Wokwi (requer extensão Wokwi no VS Code ou wokwi-cli)
## Acontece que o wokwi não suporta o partitions.csv https://github.com/wokwi/wokwi-features/issues/523
## Então tem que usar o espflash para gerar a imagem final tudo em um único binário (merge) e depois usar esse binário no Wokwi
simulate: build-rust
	espflash save-image --flash-size $(FLASH_SIZE) --chip $(MCU) --merge --partition-table $(PARTITION_TABLE) $(ELF) target/wokwi_merged.bin
	echo '{"flash_settings": {"flash_size": "$(FLASH_SIZE)"}, "flash_files": {"0x0": "wokwi_merged.bin"}}' > target/flasher_args.json
#	wokwi-cli --timeout 0 --flash $(ELF)
