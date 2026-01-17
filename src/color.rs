// Copied from my other project Sprint
// https://github.com/SamPertWasTaken/Sprint
#[derive(Clone, Copy, Debug)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8
}
impl Color {
    pub fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r, g, b, a
        }
    }
    pub fn new_mono(mono: u8, a: u8) -> Self {
        Self {
            r: mono, 
            g: mono, 
            b: mono, 
            a
        }
    }
    pub fn from_tuple(tuple: (u8, u8, u8), a: u8) -> Self {
        Self::new(tuple.0, tuple.1, tuple.2, a)
    }

    pub fn get_wayland_color(self) -> i32 {
        (i32::from(self.a) << 24) + (i32::from(self.r) << 16) + (i32::from(self.g) << 8) + i32::from(self.b)
    }
}
