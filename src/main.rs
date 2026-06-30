use rayon::prelude::*;

use image::{RgbImage, ImageBuffer};

use crossterm::{
    cursor,
    event::{self, Event, KeyCode},
    execute,
    terminal::{self},
};
use num_complex::Complex64;
use std::io::{stdout, Write};

const MAX_ITER: u32 = 1000;

struct Camera {
    center: Complex64,
    zoom: f64, // Lower value = more zoomed in
}

impl Camera {
    fn new() -> Self {
        Self {
            center: Complex64::new(-0.5, 0.0),
            zoom: 1.0,
        }
    }

    // Scale movement based on current zoom level
    fn move_center(&mut self, dx: f64, dy: f64) {
        let step = 0.1 * self.zoom;
        self.center.re += dx * step;
        self.center.im += dy * step;
    }

    fn zoom_in(&mut self) { self.zoom *= 0.9; }
    fn zoom_out(&mut self) { self.zoom *= 1.1; }
}

fn _get_rgb_values(iter: u32) -> (u8, u8, u8) {
    if iter == MAX_ITER {
        return (0, 0, 0); // Black for inside the set
    }

    let norm_t = iter as f64 / MAX_ITER as f64;
    let t = norm_t.powf(0.5) * 10.0;
    
    let r_raw = 255.0 * (0.5 * (1.0 - t + 4.0).sin() + 0.5);
    let b_raw = 255.0 * (0.5 * (1.0 - t).sin() + 0.5);
    let g_raw = 255.0 * (0.5 * (1.0 - t + 2.0).sin() + 0.5);

    let brightness = norm_t.powf(0.5); 
    
    (
        (r_raw * brightness) as u8,
        (g_raw * brightness) as u8,
        (b_raw * brightness) as u8,
    )
}

// --- Color palette ------------------------------------------------------------
//      iter    : current iteration number (0 .. MAX_ITER)
//      MAX_ITER: maximum iterations (a constant)
// Returns an RGB tuple for this point.
fn iteration_to_rgb(iter: u32, max_iter: u32) -> (u8, u8, u8) {
    // 1. Inside the set → black
    if iter == max_iter {
        return (0, 0, 0);
    }

    // 2. Normalise iteration count to [0,1]
    let norm = iter as f64 / max_iter as f64;          // < 1

    // 3. Reduce red contribution.
    //    We lower the base intensity by a constant factor (0.8),
    //    and later we apply a brightness curve that further dims it.
    const RED_FACTOR: f64 = 0.8;

    // 4. A simple HSV→RGB conversion:
    //      Hue is swept from 120° (green) → 240° (blue).
    //      Saturation and value are kept at 1.0 for a vivid colour,
    //      but we modulate the final brightness with sqrt(norm)
    let hue_deg = 120.0 + norm * 120.0;     // 120 .. 240
    let h = hue_deg / 60.0;                 // segment index [2,4]
    let f = h - (h as i32) as f64;
    let p = 0.0;
    let t = f;

    // Interpolate RGB based on hue segment
    let (r_raw, g_raw, b_raw) = if h < 3.0 {
        // segment 2 → green→cyan: R=0, G=1, B=f
        (p, t, 1.0)
    } else {
        // segment 4 → cyan→blue: R=p, G=t, B=1
        (t, 1.0, t)
    };

    // 5. Apply the RED_FACTOR and a smooth brightness curve.
    //let brightness = norm.sqrt();          // sqrt gives nicer fade at low iterations
    let brightness = norm.powf(0.5); 

    let r = (r_raw * RED_FACTOR * brightness * 255.0) as u8;
    let g = (g_raw * brightness * 255.0) as u8;
    let b = (b_raw * brightness * 255.0) as u8;

    (r, g, b)
}

/// Wrapper for the terminal renderer
fn get_rgb_color(iter: u32) -> String {
    let (r, g, b) = iteration_to_rgb(iter, MAX_ITER);
    format!("\x1b[38;2;{};{};{}m", r, g, b)
}

fn save_screenshot(cam: &Camera) -> std::io::Result<()> {
    let width = 4096;
    let height = 2304;
    let filename = "mandelbrot_screenshot_time.png";

    let x_scale = cam.zoom * (3.5 / width as f64);
    let y_scale = cam.zoom * (2.0 / height as f64);

    // Store both the RGB and the iteration count to decide where to blur
    let mut pixel_data = Vec::with_capacity(width * height);

    for y in 0..height {
        for x in 0..width {
            let re = cam.center.re + (x as f64 - width as f64 / 2.0) * x_scale;
            let im = cam.center.im + (y as f64 - height as f64 / 2.0) * y_scale;

            let c = Complex64::new(re, im);
            let mut z = Complex64::new(0.0, 0.0);
            let mut i = 0;

            while i < MAX_ITER && z.norm_sqr() <= 4.0 {
                z = z * z + c;
                i += 1;
            }

            pixel_data.push((iteration_to_rgb(i, MAX_ITER), i));
        }
    }

    // Only blur "low-end" iterations. 
    // We'll define a threshold based on the local view's max potential iterations.
    // In this case, we'll use a fixed threshold of 20% of MAX_ITER as "low-end".
    let blur_threshold = MAX_ITER / 10; 
    let blur_radius = 2;
    let mut final_pixels = Vec::with_capacity(width * height);

    for y in 0..height {
        for x in 0..width {
            let ((r, g, b), iter) = pixel_data[y * width + x];

            if false && iter > blur_threshold && iter != 0 {
                // This is a low-iteration area, apply blur
                let mut r_sum = 0u32;
                let mut g_sum = 0u32;
                let mut b_sum = 0u32;
                let mut count = 0u32;

                for dy in -(blur_radius as i32)..=(blur_radius as i32) {
                    for dx in -(blur_radius as i32)..=(blur_radius as i32) {
                        let nx = x as i32 + dx;
                        let ny = y as i32 + dy;

                        if nx >= 0 && nx < width as i32 && ny >= 0 && ny < height as i32 {
                            let ((pr, pg, pb), _) = pixel_data[ny as usize * width + nx as usize];
                            r_sum += pr as u32;
                            g_sum += pg as u32;
                            b_sum += pb as u32;
                            count += 1;
                        }
                    }
                }
                final_pixels.push(((r_sum / count) as u8, (g_sum / count) as u8, (b_sum / count) as u8));
            } else {
                // High-end iterations or the set itself: keep sharp
                final_pixels.push((r, g, b));
            }
        }
    }

    let mut img: RgbImage = ImageBuffer::new(width as u32, height as u32);
    for (i, pixel) in final_pixels.iter().enumerate() {
        let x = (i % width) as u32;
        let y = (i / width) as u32;
        img.put_pixel(x, y, image::Rgb([pixel.0, pixel.1, pixel.2]));
    }
    let _ = img.save(filename);
    println!("\nScreenshot saved (selective blur) to {}", filename);
    Ok(())
}

fn render(cam: &Camera) -> String {
    let (cols, rows) = terminal::size().unwrap_or((80, 24));
    let width = cols as f64;
    let height = rows as f64;

    let aspect_correction = 2.0;
    let x_scale = cam.zoom * (3.5 / width);
    let y_scale = cam.zoom * (2.0 / height) * aspect_correction;

    // We'll build a vector of lines, then join them at the end
    let lines: Vec<String> = (0..rows).into_par_iter().map(|y| {
        let mut line = String::with_capacity(cols as usize * 12);
        for x in 0..cols {
            let re = cam.center.re + (x as f64 - width / 2.0) * x_scale;
            let im = cam.center.im + (y as f64 - height / 2.0) * y_scale;

            let c = Complex64::new(re, im);
            let mut z = Complex64::new(0.0, 0.0);
            let mut i = 0;

            while i < MAX_ITER && z.norm_sqr() <= 4.0 {
                z = z * z + c;
                i += 1;
            }

            line.push_str(&get_rgb_color(i));
            line.push('█');
        }
        line
    }).collect();

    // Join all lines with the required CRLF sequence
    let mut buffer = String::new();
    for l in lines {
        buffer.push_str(&l);
        buffer.push('\n');
        buffer.push('\r');
    }
    buffer
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut stdout = stdout();
    let mut cam = Camera::new();

    terminal::enable_raw_mode()?;
    execute!(stdout, terminal::EnterAlternateScreen, cursor::Hide)?;

    loop {
        let frame = render(&cam);
        execute!(stdout, cursor::MoveTo(0, 0))?;
        write!(stdout, "{}", frame)?;
        write!(stdout, "\x1b[0m [WASD/Arrows]: Move | +/-: Zoom | Q: Quit | Zoom: {:.4}", cam.zoom)?;
        stdout.flush()?;

        if event::poll(std::time::Duration::from_millis(16))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('k') => {
                        save_screenshot(&cam).expect("Failed to save screenshot");
                    }
                    KeyCode::Char('+') | KeyCode::Char('=') => cam.zoom_in(),
                    KeyCode::Char('-') | KeyCode::Char('_') => cam.zoom_out(),
                    KeyCode::Up | KeyCode::Char('w') => cam.move_center(0.0, -1.0),
                    KeyCode::Down | KeyCode::Char('s') => cam.move_center(0.0, 1.0),
                    KeyCode::Left | KeyCode::Char('a') => cam.move_center(-1.0, 0.0),
                    KeyCode::Right | KeyCode::Char('d') => cam.move_center(1.0, 0.0),
                    _ => {}
                }
            }
        }
    }

    execute!(stdout, terminal::LeaveAlternateScreen, cursor::Show)?;
    terminal::disable_raw_mode()?;
    Ok(())
}
