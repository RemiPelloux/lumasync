#[derive(Default)]
pub(super) struct ScreenFrame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    pub bgra: bool,
}

impl ScreenFrame {
    pub fn rgb(&self, x: u32, y: u32) -> [u8; 3] {
        let offset = (y as usize * self.width as usize + x as usize) * 4;
        let pixel = &self.pixels[offset..offset + 3];
        if self.bgra {
            [pixel[2], pixel[1], pixel[0]]
        } else {
            [pixel[0], pixel[1], pixel[2]]
        }
    }

    pub fn set_image(&mut self, image: image::RgbaImage) {
        self.width = image.width();
        self.height = image.height();
        self.pixels = image.into_raw();
        self.bgra = false;
    }
}
