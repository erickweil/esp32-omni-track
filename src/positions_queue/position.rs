use bytemuck::{Pod, Zeroable};

/// Deve caber em 16 bytes
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C, packed)]
pub struct GPSPositionRaw {
    // 0
    /// Unix timestamp, seconds since 1970-01-01 UTC
    pub timestamp: u32,
    // 4
    /// Latitude x 1e7 (ex: 30.1234567° -> 301234567)
    pub lat: i32,
    // 8
    /// Longitude x 1e7
    pub lng: i32,
    // 12
    /// Velocidade em km/h (0-255)
    pub speed: u8,
    /// 0-360 -> 0-255
    pub course: u8,
    /// 0.1-100+ -> 0-255 Horizontal Dilution of Precision https://en.wikipedia.org/wiki/Dilution_of_precision
    pub hdop: u8,
    /// Número de satélites usados no fix
    pub sats: u8,
}