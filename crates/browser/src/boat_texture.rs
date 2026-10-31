pub struct SpriteMip {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub fn sprite_mip_chain(width: u32, height: u32, rgba: &[u8]) -> Vec<SpriteMip> {
    let linear = |byte: u8| {
        let value = f32::from(byte) / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    let encode = |value: f32| {
        let srgb = if value <= 0.0031308 {
            12.92 * value
        } else {
            1.055 * value.powf(1.0 / 2.4) - 0.055
        };
        (srgb * 255.0).round() as u8
    };
    // Filter premultiplied linear light so transparent padding adds neither
    // dark fringes nor its hidden RGB to a small hull's visible color.
    let mut pixels: Vec<[f32; 4]> = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| {
            let alpha = f32::from(p[3]) / 255.0;
            [
                linear(p[0]) * alpha,
                linear(p[1]) * alpha,
                linear(p[2]) * alpha,
                alpha,
            ]
        })
        .collect();
    let (mut width, mut height) = (width, height);
    let mut levels = Vec::new();
    loop {
        levels.push(SpriteMip {
            width,
            height,
            rgba: pixels
                .iter()
                .flat_map(|p| {
                    [
                        encode(p[0]),
                        encode(p[1]),
                        encode(p[2]),
                        (p[3] * 255.0).round() as u8,
                    ]
                })
                .collect(),
        });
        if width == 1 && height == 1 {
            break;
        }
        let next_width = (width / 2).max(1);
        let next_height = (height / 2).max(1);
        let mut next = Vec::with_capacity((next_width * next_height) as usize);
        for y in 0..next_height {
            for x in 0..next_width {
                let mut average = [0.0; 4];
                // Fractional edge texels keep odd-sized images' coverage unchanged.
                let x0 = x as f32 * width as f32 / next_width as f32;
                let x1 = (x + 1) as f32 * width as f32 / next_width as f32;
                let y0 = y as f32 * height as f32 / next_height as f32;
                let y1 = (y + 1) as f32 * height as f32 / next_height as f32;
                let area = (x1 - x0) * (y1 - y0);
                for row in y0.floor() as u32..y1.ceil() as u32 {
                    for col in x0.floor() as u32..x1.ceil() as u32 {
                        let weight = (x1.min((col + 1) as f32) - x0.max(col as f32))
                            * (y1.min((row + 1) as f32) - y0.max(row as f32))
                            / area;
                        let pixel = pixels[(row * width + col) as usize];
                        for channel in 0..4 {
                            average[channel] += pixel[channel] * weight;
                        }
                    }
                }
                next.push(average);
            }
        }
        pixels = next;
        width = next_width;
        height = next_height;
    }
    levels
}
