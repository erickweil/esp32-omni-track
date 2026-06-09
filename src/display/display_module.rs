use mipidsi::{
    Builder,
    interface::SpiInterface,
    models::ST7735s,
    options::{ColorInversion, ColorOrder, Orientation, Rotation},
    TestImage,
};
use embedded_graphics::{
    mono_font::{MonoTextStyle, ascii}, pixelcolor::Rgb565, prelude::*, primitives::Rectangle, text::Text
};

use crate::gps::GPSPosition;

pub struct DisplayModule<D> {
    display: D,
}

impl<D> DisplayModule<D> 
where 
    D: DrawTarget<Color = Rgb565>,
    D::Error: core::fmt::Debug,
{
    pub fn new(display: D) -> Self {
        Self {
            display,
        }
    }

    pub fn draw_position(&mut self, position: Option<&GPSPosition>, ip_addr: Option<std::net::IpAddr>) {
        let try_result = (|| -> Result<(), D::Error> {
            self.display.clear(Rgb565::BLACK)?;
            let text_style = MonoTextStyle::new(&ascii::FONT_7X13, Rgb565::WHITE);

            Text::new(&format!(
                "IP: {}", 
                ip_addr.unwrap_or_else(|| std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED))
            ), Point::new(2, 13), text_style).draw(&mut self.display)?;

            if let Some(pos) = position {
                let text = format!(
                    "{:}\n{:.6} {:.6}\n{:.1} km/h {:.1}o {:.1} {}",
                    pos.timestamp.unwrap_or_default(),
                    pos.latitude.unwrap_or(0.0),
                    pos.longitude.unwrap_or(0.0),
                    pos.speed.unwrap_or(0.0),
                    pos.course.unwrap_or(0.0),
                    pos.hdop.unwrap_or(0.0),
                    pos.num_satellites.unwrap_or(0),
                );
                Text::new(&text, Point::new(2, 28), text_style)
                    .draw(&mut self.display)?;
            } else {
                Text::new("Sem fix GPS!", Point::new(2, 28), text_style)
                    .draw(&mut self.display)?;
            }

            Ok(())
        })();
        if let Err(e) = try_result {
            log::info!("Erro ao desenhar no display: {:?}", e);
        }
    }
}