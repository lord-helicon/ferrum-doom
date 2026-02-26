#![allow(unsafe_code)]

use anyhow::Result;
use engine_render::{FRAME_HEIGHT, FRAME_PIXELS, FRAME_WIDTH};
use gameplay::tic_cmd::{TicCmd, ANGLETURN, FORWARDMOVE, SIDEMOVE, TIC_CMD_BUTTONS};
use gameplay::{
    m_clear_random, respawn_specials, spawn_specials, update_specials, GameAction, GameMode,
    GameOptions, Level, MapObject, PicData, Player, Skill, MAXPLAYERS,
};
use render_trait::{BufferSize, DrawBuffer, SOFT_PIXEL_CHANNELS};
use software25d::Software25D;
use sound_nosnd::{Snd, SndServerTx};
use sound_traits::{SoundServer, SoundServerTic};
use std::path::Path;
use wad::WadData;

const KEY_LEFT: u8 = 0xac;
const KEY_RIGHT: u8 = 0xae;
const KEY_UP: u8 = 0xad;
const KEY_DOWN: u8 = 0xaf;
const KEY_STRAFE_L: u8 = 0xa0;
const KEY_STRAFE_R: u8 = 0xa1;
const KEY_USE: u8 = 0xa2;
const KEY_FIRE: u8 = 0xa3;
const KEY_SPEED: u8 = 0xb6;

struct SoftFrameBuffer {
    size: BufferSize,
    buf: Vec<u8>,
    stride: usize,
}

impl SoftFrameBuffer {
    fn new(width: usize, height: usize) -> Self {
        let stride = width * SOFT_PIXEL_CHANNELS;
        Self {
            size: BufferSize::new(width, height),
            buf: vec![0; height * stride],
            stride,
        }
    }

    fn clear(&mut self, rgba: [u8; 4]) {
        for px in self.buf.chunks_exact_mut(4) {
            px.copy_from_slice(&rgba);
        }
    }
}

impl DrawBuffer for SoftFrameBuffer {
    fn size(&self) -> &BufferSize {
        &self.size
    }

    fn set_pixel(&mut self, x: usize, y: usize, colour: &[u8; 4]) {
        if x >= self.size.width_usize() || y >= self.size.height_usize() {
            return;
        }
        let idx = self.get_buf_index(x, y);
        self.buf[idx..idx + SOFT_PIXEL_CHANNELS].copy_from_slice(colour);
    }

    fn read_pixel(&self, x: usize, y: usize) -> [u8; SOFT_PIXEL_CHANNELS] {
        if x >= self.size.width_usize() || y >= self.size.height_usize() {
            return [0, 0, 0, 255];
        }
        let idx = self.get_buf_index(x, y);
        [
            self.buf[idx],
            self.buf[idx + 1],
            self.buf[idx + 2],
            self.buf[idx + 3],
        ]
    }

    fn get_buf_index(&self, x: usize, y: usize) -> usize {
        y * self.stride + x * SOFT_PIXEL_CHANNELS
    }

    fn pitch(&self) -> usize {
        self.stride
    }

    fn buf_mut(&mut self) -> &mut [u8] {
        &mut self.buf
    }

    fn debug_flip_and_present(&mut self) {}
}

pub struct DoomCore {
    wad_data: WadData,
    game_mode: GameMode,
    options: GameOptions,
    players_in_game: Box<[bool; MAXPLAYERS]>,
    players: Box<[Player; MAXPLAYERS]>,
    pic_data: PicData,
    level: Option<Level>,
    renderer: Software25D,
    draw: SoftFrameBuffer,
    framebuffer: Box<[u32]>,
    key_state: [bool; 256],
    mouse_dx: i32,
    mouse_dy: i32,
    sound: Snd,
    snd_tx: SndServerTx,
}

impl DoomCore {
    pub fn create(args: &[String]) -> Result<Self> {
        let iwad = parse_iwad_arg(args).unwrap_or_else(|| "DOOM.WAD".to_string());
        let wad_data = WadData::new(Path::new(&iwad));
        let game_mode = detect_game_mode(&wad_data);

        let mut options = GameOptions::default();
        options.iwad = iwad;
        options.skill = Skill::Medium;
        options.episode = 1;
        options.map = 1;
        options.warp = true;
        options.hi_res = true;
        options.autostart = true;

        let mut players_in_game = Box::new([false; MAXPLAYERS]);
        players_in_game[0] = true;
        let players = Box::new(std::array::from_fn(|_| Player::default()));

        let mut sound = Snd::new(&wad_data)
            .map_err(|e| anyhow::anyhow!("failed to create nosnd server: {e}"))?;
        let snd_tx = sound
            .init()
            .map_err(|e| anyhow::anyhow!("failed to initialize nosnd channel: {e}"))?;

        let pic_data = PicData::init(&wad_data);

        let mut core = Self {
            wad_data,
            game_mode,
            options,
            players_in_game,
            players,
            pic_data,
            level: None,
            renderer: Software25D::new(
                90f32.to_radians(),
                FRAME_WIDTH as f32,
                FRAME_HEIGHT as f32,
                true,
                false,
            ),
            draw: SoftFrameBuffer::new(FRAME_WIDTH, FRAME_HEIGHT),
            framebuffer: vec![0u32; FRAME_PIXELS].into_boxed_slice(),
            key_state: [false; 256],
            mouse_dx: 0,
            mouse_dy: 0,
            sound,
            snd_tx,
        };

        m_clear_random();
        core.load_current_level();
        Ok(core)
    }

    pub fn key_event(&mut self, pressed: bool, key: u8) {
        self.key_state[key as usize] = pressed;
    }

    pub fn mouse_event(&mut self, dx: i32, dy: i32) {
        self.mouse_dx = self.mouse_dx.saturating_add(dx);
        self.mouse_dy = self.mouse_dy.saturating_add(dy);
    }

    pub fn tick(&mut self) {
        self.apply_input();

        if let Some(level) = self.level.as_mut() {
            for i in 0..MAXPLAYERS {
                if self.players_in_game[i] {
                    let _ = self.players[i].think(level);
                }
            }

            unsafe {
                let lev = &mut *(level as *mut Level);
                level.thinkers.run_thinkers(lev);
            }

            level.level_time = level.level_time.saturating_add(1);
            update_specials(level, &mut self.pic_data);
            respawn_specials(level);

            if let Some(action) = level.game_action.take() {
                self.handle_game_action(action);
            }
        }

        self.sound.tic();
        self.render_frame();
    }

    pub fn framebuffer_ptr(&self) -> *const u32 {
        self.framebuffer.as_ptr()
    }

    pub fn framebuffer_words(&self) -> &[u32] {
        &self.framebuffer
    }

    fn apply_input(&mut self) {
        let mut cmd = TicCmd::new();
        let speed = usize::from(self.key_state[KEY_SPEED as usize]);

        if self.key_state[KEY_UP as usize] {
            cmd.forwardmove = cmd.forwardmove.saturating_add(FORWARDMOVE[speed] as i8);
        }
        if self.key_state[KEY_DOWN as usize] {
            cmd.forwardmove = cmd.forwardmove.saturating_sub(FORWARDMOVE[speed] as i8);
        }
        if self.key_state[KEY_STRAFE_L as usize] {
            cmd.sidemove = cmd.sidemove.saturating_sub(SIDEMOVE[speed] as i8);
        }
        if self.key_state[KEY_STRAFE_R as usize] {
            cmd.sidemove = cmd.sidemove.saturating_add(SIDEMOVE[speed] as i8);
        }

        if self.key_state[KEY_LEFT as usize] {
            cmd.angleturn = cmd.angleturn.saturating_add(ANGLETURN[1]);
        }
        if self.key_state[KEY_RIGHT as usize] {
            cmd.angleturn = cmd.angleturn.saturating_sub(ANGLETURN[1]);
        }

        cmd.angleturn = cmd
            .angleturn
            .saturating_sub((self.mouse_dx.saturating_mul(8)) as i16);

        if self.key_state[KEY_FIRE as usize] {
            cmd.buttons |= TIC_CMD_BUTTONS.bt_attack;
        }
        if self.key_state[KEY_USE as usize] {
            cmd.buttons |= TIC_CMD_BUTTONS.bt_use;
        }

        let look = (self.mouse_dy / 2).clamp(-15, 15) as i16;
        cmd.lookdir = look;

        self.players[0].cmd = cmd;
        self.mouse_dx = 0;
        self.mouse_dy = 0;
    }

    fn render_frame(&mut self) {
        self.draw.clear([0, 0, 0, 255]);

        if let Some(level) = self.level.as_ref() {
            if self.players[0].mobj().is_some() {
                self.renderer.draw_view(
                    &self.players[0],
                    level,
                    &mut self.pic_data,
                    &mut self.draw,
                );
            }
        }

        for (idx, px) in self.draw.buf.chunks_exact(4).enumerate() {
            self.framebuffer[idx] = u32::from_le_bytes([px[0], px[1], px[2], px[3]]);
        }
    }

    fn map_name(&self) -> String {
        if matches!(self.game_mode, GameMode::Commercial) {
            format!("MAP{:02}", self.options.map)
        } else {
            format!("E{}M{}", self.options.episode, self.options.map)
        }
    }

    fn load_current_level(&mut self) {
        let map_name = self.map_name();

        let mut level = unsafe {
            Level::new_empty(
                self.options.clone(),
                self.game_mode,
                self.snd_tx.clone(),
                &self.players_in_game,
                &mut self.players,
            )
        };

        level.load(
            &map_name,
            self.game_mode,
            &mut self.pic_data,
            &self.wad_data,
        );

        let things = level.map_data.things().to_vec();
        for thing in &things {
            MapObject::p_spawn_map_thing(
                *thing,
                self.options.no_monsters,
                &mut level,
                &mut self.players[..],
                &self.players_in_game,
            );
        }
        spawn_specials(&mut level);

        self.level = Some(level);
    }

    fn handle_game_action(&mut self, action: GameAction) {
        match action {
            GameAction::CompletedLevel | GameAction::WorldDone => {
                if matches!(self.game_mode, GameMode::Commercial) {
                    self.options.map = if self.options.map >= 32 {
                        1
                    } else {
                        self.options.map + 1
                    };
                } else {
                    self.options.map = if self.options.map >= 9 {
                        1
                    } else {
                        self.options.map + 1
                    };
                }
                self.load_current_level();
            }
            GameAction::Victory => {
                self.options.map = 1;
                self.load_current_level();
            }
            GameAction::LoadLevel => self.load_current_level(),
            _ => {}
        }
    }
}

fn parse_iwad_arg(args: &[String]) -> Option<String> {
    let mut i = 0usize;
    while i < args.len() {
        if args[i] == "-iwad" && i + 1 < args.len() {
            return Some(args[i + 1].clone());
        }
        i += 1;
    }
    None
}

fn detect_game_mode(wad: &WadData) -> GameMode {
    if wad.lump_exists("MAP01") {
        GameMode::Commercial
    } else if wad.lump_exists("E4M1") {
        GameMode::Retail
    } else if wad.lump_exists("E3M1") {
        GameMode::Registered
    } else {
        GameMode::Shareware
    }
}
