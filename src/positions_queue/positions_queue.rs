use std::{fs::{File, OpenOptions}, io::{Result, Write}, sync::LazyLock};

use crate::positions_queue::position::GPSPositionRaw;

const POSITION_SIZE: usize = std::mem::size_of::<GPSPositionRaw>();
pub struct QueueConfig {
    /// Número de arquivos de slot rotativos.
    pub queue_files: usize,
    /// Tamanho máximo de cada arquivo, em bytes.
    pub max_file_size: usize,
    /// Tamanho do bloco de escrita SPI Flash (para static_assert de alinhamento).
    pub spi_write_chunk: usize,
    /// Quantas posições cabem em cada arquivo (derivado de max_file_size / POSITION_SIZE).
    pub positions_per_file: usize,
}

static QUEUE_CONFIG: LazyLock<QueueConfig> = LazyLock::new(|| {
    let mut conf = QueueConfig {
        queue_files: option_env!("QUEUE_FILES").and_then(|s| s.parse().ok()).unwrap_or(8),
        max_file_size: option_env!("QUEUE_MAX_FILE_SIZE").and_then(|s| s.parse().ok()).unwrap_or(4096),
        spi_write_chunk: option_env!("SPI_WRITE_CHUNK").and_then(|s| s.parse().ok()).unwrap_or(256),
        positions_per_file: 0, // será calculado abaixo
    };

    // Para garantir que a configuração é válida
    assert!(conf.max_file_size % conf.spi_write_chunk == 0, "QUEUE_MAX_FILE_SIZE deve ser múltiplo do tamanho do bloco de escrita do SPI Flash");
    assert!(conf.max_file_size % POSITION_SIZE == 0, "QUEUE_MAX_FILE_SIZE deve ser múltiplo do tamanho de GPSPositionRaw");
    assert!(conf.queue_files >= 2 && conf.queue_files < 1024, "QUEUE_FILES deve ser maior que 2 e no máximo 1024");

    conf.positions_per_file = conf.max_file_size / POSITION_SIZE;
    assert!(conf.positions_per_file > 3, "Cada arquivo deve ser capaz de armazenar mais de 3 posições");

    conf
});

pub struct PositionQueue {
    /// Índice do início global da fila. É um índice com a primeira posição armazenada
    inicio: usize,
    /// Índice do fim global da fila. É o índice onde a próxima posição será escrita
    fim: usize,
    /// Índice de posições confirmadas como enviadas.
    fim_envio: usize,
    /// tamanho da fila, em número de posições. (a capacidade real é total_capacity - 1, pois deve sempre ter um slot vazio)
    tamanho: usize,

    inicializado: bool,
}

impl PositionQueue {
    pub fn new() -> Self {
        PositionQueue {
            inicio: 0,
            fim: 0,
            fim_envio: 0,
            tamanho: QUEUE_CONFIG.positions_per_file * QUEUE_CONFIG.queue_files,
            inicializado: false,
        }
    }

    pub fn begin(&mut self) -> Result<()> {
        if self.inicializado {
            return Ok(());
        }

        // A FAZER: 1. validar que o tamanho total disponível é suficiente
        // https://github.com/littlefs-project/littlefs/issues/1067
        // let total_queue_capacity = QUEUE_CONFIG.queue_files * QUEUE_CONFIG.max_file_size;
        // let total_fs_bytes = 

        // 2. Ler todos os arquivos e encontrar o início e fim
        let mut inicio_slot = 0;
        let mut fim_slot = usize::MAX;
        let mut fim_index = 0;
        for i in 0..QUEUE_CONFIG.queue_files {
            let file_path = Self::get_file_path(i);

            match File::open(&file_path) {
                Ok(f) => {
                    let metadata = f.metadata()?;
                    let file_size = metadata.len() as usize;

                    log::info!("Arquivo {}: size {} bytes", file_path, file_size);

                    if fim_slot == usize::MAX {
                        if file_size < POSITION_SIZE {
                            fim_slot = i;
                            fim_index = file_size / POSITION_SIZE;
                        }
                    } else if inicio_slot == 0 {
                        // Após encontrar o primeiro arquivo não cheio, o próximo que existir é o início
                        inicio_slot = i;
                    }
                },
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    // O anterior está cheio e este não existe, então o anterior é o último com dados e este é o fim
                    if fim_slot == usize::MAX {
                        fim_slot = i;
                        fim_index = 0;
                    }
                },
                Err(e) => {
                    return Err(e);
                }
            }
        }

        if fim_slot == usize::MAX {
            log::info!("Não encontrou uma fila válida no sistema de arquivos, iniciando nova fila vazia");
            
            fim_slot = 0;
            fim_index = 0;
            inicio_slot = 1;
        }

        self.fim = fim_slot * QUEUE_CONFIG.positions_per_file + fim_index;
        self.inicio = inicio_slot * QUEUE_CONFIG.positions_per_file + 0;
        self.fim_envio = self.inicio;

        // A FAZER: persistir fim_envio de algum forma

        self.inicializado = true;
        Ok(())
    }

    pub fn size(&self) -> usize {
        (self.tamanho - self.inicio + self.fim) % self.tamanho
    }

    pub fn pending_send(&self) -> usize {
        (self.tamanho - self.fim_envio + self.fim) % self.tamanho
    }

    pub fn capacity(&self) -> usize {
        self.tamanho - 1
    }

    pub fn is_empty(&self) -> bool {
        self.inicio == self.fim
    }

    pub fn get_start(&self) -> usize {
        self.inicio
    }

    pub fn get_send_index(&self) -> usize {
        self.fim_envio
    }

    pub fn get_end(&self) -> usize {
        self.fim
    }

    /// Irá marcar que todas as posições até SendIndex foram enviadas/consumidas.
    pub fn commit_send(&mut self, index: usize) {
        self.fim_envio = index;
    }

    pub fn enqueue(&mut self, position: &GPSPositionRaw) -> Result<()> {
        self.write_position_to_file(self.fim, position)?;
        self.fim = self.increment_index(self.fim);
        Ok(())
    }

    // A FAZER: ler posições. Iterator?

    // ----------------------------------------------------------
    // Privados
    // ----------------------------------------------------------

    #[inline(always)]
    fn increment_index(&self, index: usize) -> usize {
        (index + 1) % self.tamanho
    }

    fn map_position_to_file(&self, index: usize) -> (usize, usize) {
        let file_slot = (index / QUEUE_CONFIG.positions_per_file) % QUEUE_CONFIG.queue_files;
        let file_index = index % QUEUE_CONFIG.positions_per_file;
        (file_slot, file_index)
    }

    fn get_file_path(slot: usize) -> String {
        format!("/positions.{}.bin", slot)
    }

    fn write_position_to_file(&mut self, index: usize, position: &GPSPositionRaw) -> Result<()> {
        let (file_slot, local_index) = self.map_position_to_file(index);
        let path = Self::get_file_path(file_slot);

        // Abre em append (cria se não existir)
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;

        // Garante que estamos escrevendo exatamente no offset esperado
        let expected_offset = local_index * POSITION_SIZE;
        let actual_size = f.metadata()?.len() as usize;
        if actual_size != expected_offset {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "Offset inesperado no arquivo {}: esperado {}, encontrado {}",
                    path, expected_offset, actual_size
                ),
            ));
        }

        // Serializa o Record como bytes, usando bytemuck
        let bytes = bytemuck::bytes_of(position);
        f.write_all(bytes)?;

        // Se o arquivo ficou cheio, deleta o próximo slot e avança os índices
        if actual_size + POSITION_SIZE >= QUEUE_CONFIG.max_file_size {
            let next_slot = (file_slot + 1) % QUEUE_CONFIG.queue_files;
            let next_path = Self::get_file_path(next_slot);
            std::fs::remove_file(&next_path).ok(); // ignora erro se não existir

            // Avança inicio e fim_envio se estavam no slot deletado
            let novo_inicio = ((next_slot + 1) % QUEUE_CONFIG.queue_files)
                * QUEUE_CONFIG.positions_per_file;

            let (inicio_slot, _) = self.map_position_to_file(self.inicio);
            if inicio_slot == next_slot {
                self.inicio = novo_inicio;
            }

            let (envio_slot, _) = self.map_position_to_file(self.fim_envio);
            if envio_slot == next_slot {
                self.fim_envio = novo_inicio;
            }
        }

        Ok(())
    }
}