#![deny(unsafe_code)]

use anyhow::Result;
use engine_render::{FRAME_HEIGHT, FRAME_PIXELS, FRAME_WIDTH};

pub const FRACBITS: i32 = 16;
pub const FRACUNIT: i32 = 1 << FRACBITS;

const KEY_LEFT: u8 = 0xac;
const KEY_RIGHT: u8 = 0xae;
const KEY_UP: u8 = 0xad;
const KEY_DOWN: u8 = 0xaf;
const KEY_STRAFE_L: u8 = 0xa0;
const KEY_STRAFE_R: u8 = 0xa1;
const KEY_SPEED: u8 = 0xb6;

const MAP_W: usize = 16;
const MAP_H: usize = 16;
const MAP: [u8; MAP_W * MAP_H] = [
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
    1, 0, 1, 1, 1, 1, 0, 0, 0, 1, 1, 1, 1, 1, 0, 1, 1, 0, 1, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 1,
    1, 0, 1, 0, 0, 1, 0, 1, 0, 1, 0, 0, 0, 1, 0, 1, 1, 0, 1, 1, 0, 1, 0, 1, 0, 1, 0, 1, 1, 1, 0, 1,
    1, 0, 0, 1, 0, 0, 0, 1, 0, 1, 0, 0, 0, 0, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 0, 1, 1, 1, 1, 1, 0, 1,
    1, 0, 0, 0, 0, 1, 0, 1, 0, 0, 0, 0, 0, 1, 0, 1, 1, 0, 1, 1, 0, 1, 0, 1, 1, 1, 1, 1, 0, 1, 0, 1,
    1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 1, 0, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 0, 1, 0, 1, 0, 1,
    1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 1, 1, 0, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 0, 1, 0, 1,
    1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
];

#[derive(Clone)]
pub struct DoomCore {
    framebuffer: Box<[u32]>,
    tic: u64,
    pos_x: i32,
    pos_y: i32,
    angle_deg: i32,
    keys: [bool; 256],
    mouse_dx: i32,
}

impl DoomCore {
    pub fn create(_args: &[String]) -> Result<Self> {
        Ok(Self {
            framebuffer: vec![0u32; FRAME_PIXELS].into_boxed_slice(),
            tic: 0,
            pos_x: 3 * FRACUNIT,
            pos_y: 3 * FRACUNIT,
            angle_deg: 0,
            keys: [false; 256],
            mouse_dx: 0,
        })
    }

    pub fn key_event(&mut self, pressed: bool, key: u8) {
        self.keys[key as usize] = pressed;
    }

    pub fn mouse_event(&mut self, dx: i32, _dy: i32) {
        self.mouse_dx = self.mouse_dx.saturating_add(dx);
    }

    pub fn tick(&mut self) {
        self.tic = self.tic.wrapping_add(1);

        let mut turn = 0i32;
        if self.keys[KEY_LEFT as usize] {
            turn -= 4;
        }
        if self.keys[KEY_RIGHT as usize] {
            turn += 4;
        }
        turn += self.mouse_dx / 2;
        self.mouse_dx = 0;

        self.angle_deg = (self.angle_deg + turn).rem_euclid(360);

        let speed = if self.keys[KEY_SPEED as usize] { 7 } else { 4 } * FRACUNIT / 35;
        let mut move_forward = 0i32;
        if self.keys[KEY_UP as usize] {
            move_forward += speed;
        }
        if self.keys[KEY_DOWN as usize] {
            move_forward -= speed;
        }
        let mut move_strafe = 0i32;
        if self.keys[KEY_STRAFE_L as usize] {
            move_strafe -= speed;
        }
        if self.keys[KEY_STRAFE_R as usize] {
            move_strafe += speed;
        }

        self.apply_movement(move_forward, move_strafe);
        self.render_frame();
    }

    pub fn framebuffer_ptr(&self) -> *const u32 {
        self.framebuffer.as_ptr()
    }

    pub fn framebuffer_words(&self) -> &[u32] {
        &self.framebuffer
    }

    fn apply_movement(&mut self, forward: i32, strafe: i32) {
        if forward == 0 && strafe == 0 {
            return;
        }

        let a = (self.angle_deg as f32).to_radians();
        let sin_a = a.sin();
        let cos_a = a.cos();

        let dx = (forward as f32 * cos_a - strafe as f32 * sin_a) as i32;
        let dy = (forward as f32 * sin_a + strafe as f32 * cos_a) as i32;

        let next_x = self.pos_x.saturating_add(dx);
        let next_y = self.pos_y.saturating_add(dy);

        if !is_wall(next_x, self.pos_y) {
            self.pos_x = next_x;
        }
        if !is_wall(self.pos_x, next_y) {
            self.pos_y = next_y;
        }
    }

    fn render_frame(&mut self) {
        let horizon = (FRAME_HEIGHT as i32) / 2;

        for y in 0..FRAME_HEIGHT {
            let color = if y as i32 <= horizon {
                0xFF3A2C6Au32
            } else {
                0xFF2A1F10u32
            };
            let row = y * FRAME_WIDTH;
            for x in 0..FRAME_WIDTH {
                self.framebuffer[row + x] = color;
            }
        }

        let fov = 66.0f32;
        let angle_base = self.angle_deg as f32;

        for screen_x in 0..FRAME_WIDTH {
            let ndc = (2.0 * screen_x as f32 / FRAME_WIDTH as f32) - 1.0;
            let ray_angle = angle_base + ndc * (fov * 0.5);
            let ray_rad = ray_angle.to_radians();
            let dir_x = ray_rad.cos();
            let dir_y = ray_rad.sin();

            let dist = cast_ray(self.pos_x, self.pos_y, dir_x, dir_y);
            let corrected = dist * ((ray_angle - angle_base).to_radians().cos().abs().max(0.2));
            let wall_h = (FRAME_HEIGHT as f32 * 0.8 / corrected).clamp(2.0, FRAME_HEIGHT as f32);
            let half = (wall_h / 2.0) as i32;
            let top = (horizon - half).max(0) as usize;
            let bot = (horizon + half).min(FRAME_HEIGHT as i32 - 1) as usize;

            let shade = (255.0 / (1.0 + corrected * 0.25)).clamp(25.0, 255.0) as u32;
            let color = 0xFF000000 | (shade << 16) | ((shade / 2) << 8) | (shade / 4);

            for y in top..=bot {
                self.framebuffer[y * FRAME_WIDTH + screen_x] = color;
            }
        }

        let cx = FRAME_WIDTH / 2;
        let cy = FRAME_HEIGHT / 2;
        for dx in -8..=8 {
            let x = (cx as isize + dx) as usize;
            if x < FRAME_WIDTH {
                self.framebuffer[cy * FRAME_WIDTH + x] = 0xFFFFFFFF;
            }
        }
        for dy in -8..=8 {
            let y = (cy as isize + dy) as usize;
            if y < FRAME_HEIGHT {
                self.framebuffer[y * FRAME_WIDTH + cx] = 0xFFFFFFFF;
            }
        }
    }
}

fn cast_ray(start_x: i32, start_y: i32, dir_x: f32, dir_y: f32) -> f32 {
    let mut t = 0.0f32;
    let step = 0.02f32;

    for _ in 0..2048 {
        let x = start_x as f32 / FRACUNIT as f32 + dir_x * t;
        let y = start_y as f32 / FRACUNIT as f32 + dir_y * t;

        let ix = x.floor() as i32;
        let iy = y.floor() as i32;

        if ix < 0 || iy < 0 || ix >= MAP_W as i32 || iy >= MAP_H as i32 {
            return 32.0;
        }

        if MAP[iy as usize * MAP_W + ix as usize] != 0 {
            return t.max(0.05);
        }

        t += step;
    }

    32.0
}

fn is_wall(x: i32, y: i32) -> bool {
    let ix = x >> FRACBITS;
    let iy = y >> FRACBITS;

    if ix < 0 || iy < 0 || ix >= MAP_W as i32 || iy >= MAP_H as i32 {
        return true;
    }

    MAP[iy as usize * MAP_W + ix as usize] != 0
}
