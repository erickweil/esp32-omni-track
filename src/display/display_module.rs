use esp32_omni_track::Result;

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
    phantom: core::marker::PhantomData<D>,
}

impl<D> DisplayModule<D> 
where 
    D: DrawTarget<Color = Rgb565>,
    D::Error: core::fmt::Debug,
{
    pub fn new() -> Self {
        Self {
            phantom: core::marker::PhantomData,
        }
    }

    pub fn draw_position(&mut self, display: &mut D, position: Option<&GPSPosition>) -> Result<()> {
        display.clear(Rgb565::BLACK)
            .map_err(|e| format!("Erro ao limpar display: {:?}", e))?;
        
        if let Some(pos) = position {
            let text_style = MonoTextStyle::new(&ascii::FONT_7X13, Rgb565::WHITE);
            let text = format!(
                "{:?}\n{:.6}\n{:.6}\n{:.1} km/h {:.1}o {:.1} {}",
                pos.timestamp.unwrap_or_default(),
                pos.latitude.unwrap_or(0.0),
                pos.longitude.unwrap_or(0.0),
                pos.speed.unwrap_or(0.0),
                pos.course.unwrap_or(0.0),
                pos.hdop.unwrap_or(0.0),
                pos.num_satellites.unwrap_or(0),
            );
            Text::new(&text, Point::new(5, 20), text_style)
                .draw(display)
                .map_err(|e| format!("Erro ao desenhar texto: {:?}", e))?;
        } else {
            let text_style = MonoTextStyle::new(&ascii::FONT_10X20, Rgb565::WHITE);
            Text::new("Sem fix GPS!", Point::new(5, 20), text_style)
                .draw(display)
                .map_err(|e| format!("Erro ao desenhar texto: {:?}", e))?;
        }

        Ok(())
    }
}