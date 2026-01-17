use crate::color::Color;

pub struct ColorRamp {
    points: Vec<Color>,
    gradient: Vec<Color>
}
impl ColorRamp {
    pub fn new(points: Vec<Color>) -> Self {
        let gradient: Vec<Color> = ColorRamp::polylinear_gradient(points.clone());
        println!("{gradient:?}");
        println!("{}", gradient.len());

        Self {
            points,
            gradient
        }
    }

    // Linear gradient, described by https://bsouthga.dev/posts/color-gradients-with-python
    fn linear_gradient(starting: Color, ending: Color, resolution: u32) -> Vec<Color> {
        let mut result: Vec<Color> = Vec::new();

        for x in 0..resolution {
            let r = (starting.r as f32 + ((x as f32) / (resolution as f32 - 1.0) * ((ending.r - starting.r) as f32))) as u8;
            let g = (starting.g as f32 + ((x as f32) / (resolution as f32 - 1.0) * ((ending.g - starting.g) as f32))) as u8;
            let b = (starting.b as f32 + ((x as f32) / (resolution as f32 - 1.0) * ((ending.b - starting.b) as f32))) as u8;
            result.push(Color::new(r, g, b, 255));
        }

        result
    }
    // Polylinear gradient, described by https://bsouthga.dev/posts/color-gradients-with-python
    pub fn polylinear_gradient(colors: Vec<Color>) -> Vec<Color> {
        let resolution = 64;
        let per_gradient_res = (resolution as f32 / (colors.len() as f32 - 1.0)) as u32;
        let mut result: Vec<Color> = ColorRamp::linear_gradient(colors[0], colors[1], per_gradient_res);
        
        for i in 1..colors.len() - 1 {
            let mut next_gradient: Vec<Color> = ColorRamp::linear_gradient(colors[i], colors[i + 1], per_gradient_res);
            let _ = next_gradient.iter().take(1);
            result.append(&mut next_gradient);
        }

        result
    }

    pub fn get_color_at_point(&self, point: f32) -> Color {
        let index = (point * ((self.gradient.len() - 1) as f32)).round() as u32;
        self.gradient[index as usize]
    }
}
