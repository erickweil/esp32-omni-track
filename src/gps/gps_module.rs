use std::io::Read;
use chrono::NaiveDateTime;
use nmea::{Nmea, sentences::FixType};
use esp32_omni_track::Result;

use crate::gps::LineByLineIterator;

#[derive(Clone)]
pub struct GPSPosition {
    pub timestamp: Option<NaiveDateTime>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub speed: Option<f32>,
    pub course: Option<f32>,
    pub hdop: Option<f32>,
    pub num_satellites: Option<u32>,
}

pub struct GPSModule<U> {
    uart: U,
    nmea_parser: Box<Nmea>,
    line_iterator: LineByLineIterator
}

impl<U> GPSModule<U> 
where U: Read
{
    pub fn new(uart: U) -> Self {
        Self {
            // Criar na HEAP, isso consome memória demais!
            // https://github.com/AeroRust/nmea/issues/2
            // TODO: pesquisar outra biblioteca mais leve? o TinyGPS++ fazia como? 
            uart,
            nmea_parser: Box::default(),
            line_iterator: LineByLineIterator::new(),
        }
    }

    pub fn read_from_uart(&mut self) -> Result<bool> {
        let mut changed = false;
        loop {
            let bytes_read = self.line_iterator.fill_from(&mut self.uart)?;
            
            self.line_iterator.drain_lines(|line| {
                log::info!("GPS '{}'", line);

                // Tenta parsear a linha como uma sentença NMEA
                if self.nmea_parser.parse(line).is_ok() {
                    changed = true;
                }
            });

            if bytes_read == 0 {
                break; // Sem mais dados disponíveis no momento
            }
        }

        Ok(changed)
    }

    pub fn get_last_fix(&self) -> Option<GPSPosition> {
        match self.nmea_parser.fix_type() {
            None | Some(FixType::Invalid) => None,
            _ => Some(GPSPosition {
                timestamp: if let Some(date) = self.nmea_parser.fix_date && let Some(time) = self.nmea_parser.fix_time {
                    Some(NaiveDateTime::new(date, time))
                } else {
                    None
                },
                latitude: self.nmea_parser.latitude(),
                longitude: self.nmea_parser.longitude(),
                // Speed over ground, knots
                // The knot is a unit of speed equal to one nautical mile per hour, exactly 1.852 km/h
                speed: self.nmea_parser.speed_over_ground.map(|knots| knots * 1.852),
                course: self.nmea_parser.true_course,
                hdop: self.nmea_parser.hdop,
                num_satellites: self.nmea_parser.fix_satellites(),
            })
        }
    }
}


#[cfg(test)]
mod tests {
    use nmea::sentences::FixType;
    use super::*;

    // https://swairlearn.bluecover.pt/nmea_analyser
    const GPS_STREAM: &str = "$GPRMC,045103.000,A,3014.1984,N,09749.2872,W,0.67,161.46,030913,,,A*7C\r\n\
$GPGGA,045104.000,3014.1985,N,09749.2873,W,1,09,1.2,211.6,M,-22.5,M,,0000*62\r\n\
$GPRMC,045200.000,A,3014.3820,N,09748.9514,W,36.88,65.02,030913,,,A*77\r\n\
$GPGGA,045201.000,3014.3864,N,09748.9411,W,1,10,1.2,200.8,M,-22.5,M,,0000*6C\r\n\
$GPRMC,045251.000,A,3014.4275,N,09749.0626,W,0.51,217.94,030913,,,A*7D\r\n\
$GPGGA,045252.000,3014.4273,N,09749.0628,W,1,09,1.3,206.9,M,-22.5,M,,0000*6F\r\n";

    #[test_log::test]
    fn test_gps_module() {
        let reader = std::io::Cursor::new(GPS_STREAM.as_bytes().to_vec());
        let mut gps_module = GPSModule::new(reader);
        loop {
            let changed = gps_module.read_from_uart()
                .expect("Failed to read from GPS module");

            if changed {
                gps_module.get_last_fix().expect("Expected a valid GPS fix");
            } else {
                break;
            }
        }
        assert_eq!(gps_module.uart.position(), GPS_STREAM.len() as u64);

        assert_eq!(gps_module.nmea_parser.fix_type(), Some(FixType::Gps));
        let fix = gps_module.get_last_fix().expect("Expected a valid GPS fix");
        assert!(fix.latitude.is_some());
        assert!(fix.longitude.is_some());
    }
}