use esp32_omni_track::Result;

/// Limite máximo do buffer acumulado. Se ultrapassado, os dados são descartados
/// Uma linha GPS típica tem cerca de 80 caracteres, então 512 é um buffer razoável para acumular várias linhas antes de processar.
const MAX_BUFFER: usize = 512;

pub struct LineByLineIterator {
    /// Buffer de leitura
    buf: Box::<[u8]>,
    /// Quantos bytes no 'buf' são válidos
    fill: usize,
}

impl LineByLineIterator {
    pub fn new() -> Self {
        Self { 
            buf: vec![0u8; MAX_BUFFER].into_boxed_slice(),
            fill: 0 
        }
    }

    /// Chama `reader` passando o espaço livre do buffer e incorpora os bytes escritos.
    pub fn fill_from(&mut self, reader: &mut impl std::io::Read) -> Result<usize> {
        if self.fill >= MAX_BUFFER {
            log::warn!("Line buffer filled up");
            // Descarta o conteúdo antigo e lê os novos dados.
            self.fill = 0;
        }

        let n = reader.read(&mut self.buf[self.fill..])?;
        self.fill += n;
        Ok(n)
    }

    /// Atravessa o buffer procurando por linhas completas (terminadas em \n) e chama a callback
    pub fn drain_lines(&mut self, mut f: impl FnMut(&str)) {
        loop {
            if self.fill == 0 { break; }
            let Some(pos) = self.buf[..self.fill].iter().position(|&b| b == b'\n') else {
                break; // Sem linha completa disponível
            };

            // Encontra o fim real da linha (sem \r)
            let end = if pos > 0 && self.buf[pos - 1] == b'\r' { pos - 1 } else { pos };
            if end > 0 && let Ok(line) = str::from_utf8(&self.buf[..end]) {
                f(line);
            }
            
            // Desloca os bytes restantes para o início (sem realocar).
            self.buf.copy_within(pos + 1..self.fill, 0);
            self.fill -= pos + 1;
        }
    }

    #[cfg(test)]
    pub fn bytes_available(&self) -> usize {
        self.buf.len() - self.fill
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_line_by_line_iterator() {
        // The idea is to test if it can handle incomplete lines, byte by byte and multiple lines at once.
        let test_input = b"line1\r\nline2\nline3\r\nline\r4\r\npartial";

        // 1. Multiple lines at once
        let mut iterator = LineByLineIterator::new();
        let mut output = Vec::new();
        iterator.fill_from(&mut std::io::Cursor::new(test_input)).unwrap();
        iterator.drain_lines(|line| output.push(line.to_string()));
        assert_eq!(output, vec!["line1", "line2", "line3", "line\r4"]);

        // 2. Byte by byte
        let mut iterator = LineByLineIterator::new();
        let mut output = Vec::new();
        for &byte in test_input {
            iterator.fill_from(&mut std::io::Cursor::new([byte])).unwrap();
            iterator.drain_lines(|line| output.push(line.to_string()));
        }
        assert_eq!(output, vec!["line1", "line2", "line3", "line\r4"]);
    }

    #[test]
    fn test_buffer_overflow() {
        let mut iterator = LineByLineIterator::new();
        let large_input = vec![b'A'; MAX_BUFFER + 100]; // Excede o buffer
        let bytes_read = iterator.fill_from(&mut std::io::Cursor::new(large_input)).unwrap();
        assert_eq!(bytes_read, MAX_BUFFER);
        assert_eq!(iterator.bytes_available(), 0); // O buffer deve estar cheio
    }
}