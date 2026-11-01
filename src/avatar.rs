// src/avatar.rs
// Sistema de avatar usando sprite sheets estilo Capri (PNG animados con dimensiones reales 240x384 y 384x384)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Runtime};

/// Estados del avatar (mapeados a sprites de Capri)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AvatarState {
    Idle,
    Talking,
    Listening,
    Thinking,
    Happy,
    Sad,
    Glitch,
    Sleep,
    Dragged,
    Sitting,
    Napping,
    Dance,
    Wander,
    Arrive,
    Leave,
    Wake,
}

/// Configuración de sprite sheet
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpriteSheetConfig {
    pub file: String,
    pub frame_width: u32,
    pub frame_height: u32,
    pub frames: u32,
    pub frame_duration_ms: u32,
    pub loop_animation: bool,
}

/// Configuración del avatar
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvatarConfig {
    pub size: u32,
    pub position: (f64, f64),
    pub current_state: AvatarState,
    pub sprites: HashMap<AvatarState, SpriteSheetConfig>,
}

/// Sistema principal del avatar con sprite sheets
pub struct AvatarSystem<R: Runtime> {
    app_handle: AppHandle<R>,
    config: Arc<Mutex<AvatarConfig>>,
    animation_handle: Option<tauri::async_runtime::JoinHandle<()>>,
    current_frame: Arc<Mutex<u32>>,
}

impl<R: Runtime> AvatarSystem<R> {
    fn capri_sprites() -> HashMap<AvatarState, SpriteSheetConfig> {
        let mut map = HashMap::new();

        map.insert(
            AvatarState::Idle,
            SpriteSheetConfig {
                file: "sprites/idle.png".to_string(),
                frame_width: 240,
                frame_height: 384,
                frames: 16,
                frame_duration_ms: 95,
                loop_animation: true,
            },
        );

        map.insert(
            AvatarState::Talking,
            SpriteSheetConfig {
                file: "sprites/mouth.png".to_string(),
                frame_width: 240,
                frame_height: 384,
                frames: 5,
                frame_duration_ms: 90,
                loop_animation: true,
            },
        );

        map.insert(
            AvatarState::Listening,
            SpriteSheetConfig {
                file: "sprites/blink.png".to_string(),
                frame_width: 240,
                frame_height: 384,
                frames: 4,
                frame_duration_ms: 180,
                loop_animation: true,
            },
        );

        map.insert(
            AvatarState::Thinking,
            SpriteSheetConfig {
                file: "sprites/blink.png".to_string(),
                frame_width: 240,
                frame_height: 384,
                frames: 4,
                frame_duration_ms: 130,
                loop_animation: true,
            },
        );

        map.insert(
            AvatarState::Happy,
            SpriteSheetConfig {
                file: "sprites/grin.png".to_string(),
                frame_width: 240,
                frame_height: 384,
                frames: 16,
                frame_duration_ms: 80,
                loop_animation: true,
            },
        );

        map.insert(
            AvatarState::Sad,
            SpriteSheetConfig {
                file: "sprites/leave.png".to_string(),
                frame_width: 240,
                frame_height: 384,
                frames: 12,
                frame_duration_ms: 110,
                loop_animation: false,
            },
        );

        map.insert(
            AvatarState::Glitch,
            SpriteSheetConfig {
                file: "sprites/glitch.png".to_string(),
                frame_width: 240,
                frame_height: 384,
                frames: 10,
                frame_duration_ms: 65,
                loop_animation: true,
            },
        );

        map.insert(
            AvatarState::Sleep,
            SpriteSheetConfig {
                file: "sprites/sleep.png".to_string(),
                frame_width: 240,
                frame_height: 384,
                frames: 16,
                frame_duration_ms: 140,
                loop_animation: true,
            },
        );

        map.insert(
            AvatarState::Dragged,
            SpriteSheetConfig {
                file: "sprites/dangle.png".to_string(),
                frame_width: 240,
                frame_height: 384,
                frames: 24,
                frame_duration_ms: 65,
                loop_animation: true,
            },
        );

        map.insert(
            AvatarState::Sitting,
            SpriteSheetConfig {
                file: "sprites/perch.png".to_string(),
                frame_width: 384,
                frame_height: 384,
                frames: 16,
                frame_duration_ms: 95,
                loop_animation: true,
            },
        );

        map.insert(
            AvatarState::Napping,
            SpriteSheetConfig {
                file: "sprites/sleep.png".to_string(),
                frame_width: 240,
                frame_height: 384,
                frames: 16,
                frame_duration_ms: 180,
                loop_animation: true,
            },
        );

        map.insert(
            AvatarState::Dance,
            SpriteSheetConfig {
                file: "sprites/dance.png".to_string(),
                frame_width: 240,
                frame_height: 384,
                frames: 16,
                frame_duration_ms: 80,
                loop_animation: true,
            },
        );

        map.insert(
            AvatarState::Wander,
            SpriteSheetConfig {
                file: "sprites/walk.png".to_string(),
                frame_width: 240,
                frame_height: 384,
                frames: 12,
                frame_duration_ms: 90,
                loop_animation: true,
            },
        );

        map.insert(
            AvatarState::Arrive,
            SpriteSheetConfig {
                file: "sprites/arrive.png".to_string(),
                frame_width: 240,
                frame_height: 384,
                frames: 14,
                frame_duration_ms: 70,
                loop_animation: false,
            },
        );

        map.insert(
            AvatarState::Leave,
            SpriteSheetConfig {
                file: "sprites/leave.png".to_string(),
                frame_width: 240,
                frame_height: 384,
                frames: 12,
                frame_duration_ms: 85,
                loop_animation: false,
            },
        );

        map.insert(
            AvatarState::Wake,
            SpriteSheetConfig {
                file: "sprites/wake.png".to_string(),
                frame_width: 240,
                frame_height: 384,
                frames: 13,
                frame_duration_ms: 85,
                loop_animation: false,
            },
        );

        map
    }

    pub fn new(app_handle: AppHandle<R>) -> Self {
        let config = AvatarConfig {
            size: 160,
            position: (0.0, 0.0),
            current_state: AvatarState::Idle,
            sprites: Self::capri_sprites(),
        };

        Self {
            app_handle,
            config: Arc::new(Mutex::new(config)),
            animation_handle: None,
            current_frame: Arc::new(Mutex::new(0)),
        }
    }

    pub fn set_state(&self, state: AvatarState) {
        {
            let mut config = self.config.lock().unwrap();
            if config.current_state != state {
                config.current_state = state;
                if let Ok(mut f) = self.current_frame.lock() {
                    *f = 0;
                }
            }
        }
        self.emit_state_change(state);
    }

    pub fn get_state(&self) -> AvatarState {
        self.config.lock().unwrap().current_state
    }

    fn emit_state_change(&self, state: AvatarState) {
        let config = self.config.lock().unwrap();
        if let Some(sprite) = config.sprites.get(&state) {
            let _ = self.app_handle.emit(
                "avatar_state_change",
                serde_json::json!({
                    "state": state,
                    "sprite": sprite.file,
                    "frames": sprite.frames,
                    "frame_duration_ms": sprite.frame_duration_ms,
                    "loop": sprite.loop_animation,
                    "frame_width": sprite.frame_width,
                    "frame_height": sprite.frame_height,
                }),
            );
        }
    }

    pub fn start_animation_loop(&mut self) {
        let app_handle = self.app_handle.clone();
        let config = self.config.clone();
        let current_frame = self.current_frame.clone();

        let handle = tauri::async_runtime::spawn(async move {
            loop {
                let next_tick = {
                    let cfg = config.lock().unwrap();
                    let state = cfg.current_state;
                    if let Some(sprite) = cfg.sprites.get(&state).cloned() {
                        let mut frame_guard = current_frame.lock().unwrap();
                        let frame = *frame_guard;
                        if sprite.loop_animation || (*frame_guard + 1) < sprite.frames {
                            *frame_guard = (*frame_guard + 1) % sprite.frames;
                        }
                        Some((state, sprite, frame))
                    } else {
                        None
                    }
                };

                let Some((state, sprite, frame)) = next_tick else {
                    tokio::time::sleep(tokio::time::Duration::from_millis(90)).await;
                    continue;
                };

                let _ = app_handle.emit(
                    "avatar_frame",
                    serde_json::json!({
                        "state": state,
                        "sprite": sprite.file,
                        "frame": frame,
                        "frames": sprite.frames,
                        "frame_width": sprite.frame_width,
                        "frame_height": sprite.frame_height,
                    }),
                );

                let delay = sprite.frame_duration_ms.max(40) as u64;
                tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
            }
        });

        self.animation_handle = Some(handle);
    }

    pub fn stop_animation(&mut self) {
        if let Some(handle) = self.animation_handle.take() {
            handle.abort();
        }
    }

    pub fn set_position(&self, x: f64, y: f64) {
        let mut config = self.config.lock().unwrap();
        config.position = (x, y);
        drop(config);
        let _ = self
            .app_handle
            .emit("avatar_position_change", serde_json::json!({ "x": x, "y": y }));
    }

    pub async fn glitch_in(&self) {
        self.set_state(AvatarState::Glitch);
        tokio::time::sleep(tokio::time::Duration::from_millis(750)).await;
        self.set_state(AvatarState::Arrive);
        tokio::time::sleep(tokio::time::Duration::from_millis(850)).await;
        self.set_state(AvatarState::Idle);

        let _ = self.app_handle.emit("avatar_spawned", true);
    }

    pub fn head_pat(&self) {
        self.set_state(AvatarState::Happy);
        let config = self.config.clone();
        let current_frame = self.current_frame.clone();
        let app_handle = self.app_handle.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(2200)).await;
            {
                let mut cfg = config.lock().unwrap();
                cfg.current_state = AvatarState::Idle;
                if let Ok(mut f) = current_frame.lock() {
                    *f = 0;
                }
            }
            let _ = app_handle.emit(
                "avatar_state_change",
                serde_json::json!({ "state": AvatarState::Idle }),
            );
        });
    }

    pub fn dance(&self) {
        self.set_state(AvatarState::Dance);
        let config = self.config.clone();
        let current_frame = self.current_frame.clone();
        let app_handle = self.app_handle.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(3800)).await;
            {
                let mut cfg = config.lock().unwrap();
                if cfg.current_state == AvatarState::Dance {
                    cfg.current_state = AvatarState::Idle;
                    if let Ok(mut f) = current_frame.lock() {
                        *f = 0;
                    }
                }
            }
            let _ = app_handle.emit(
                "avatar_state_change",
                serde_json::json!({ "state": AvatarState::Idle }),
            );
        });
    }

    pub fn start_check_ins(&self, interval_minutes: u64) {
        let app_handle = self.app_handle.clone();
        tauri::async_runtime::spawn(async move {
            let mut interval =
                tokio::time::interval(tokio::time::Duration::from_secs(interval_minutes * 60));
            interval.tick().await; // skip immediate tick
            loop {
                interval.tick().await;
                let _ = app_handle.emit(
                    "avatar_check_in",
                    serde_json::json!({
                        "message": "¡Hola Aym! ¿Cómo vas con lo que estás haciendo? 🐾",
                        "type": "check_in"
                    }),
                );
            }
        });
    }
}

pub fn init_avatar_system<R: tauri::Runtime>(app_handle: AppHandle<R>) -> AvatarSystem<R> {
    let system = AvatarSystem::new(app_handle);
    system.start_check_ins(20);
    system
}

/// Tauri commands
#[tauri::command]
pub async fn set_avatar_state(
    state: AvatarState,
    avatar: tauri::State<'_, std::sync::Arc<AvatarSystem<tauri::Wry>>>,
) -> Result<(), String> {
    avatar.set_state(state);
    Ok(())
}

#[tauri::command]
pub async fn get_avatar_state(
    avatar: tauri::State<'_, std::sync::Arc<AvatarSystem<tauri::Wry>>>,
) -> Result<AvatarState, String> {
    Ok(avatar.get_state())
}

#[tauri::command]
pub async fn head_pat(
    avatar: tauri::State<'_, std::sync::Arc<AvatarSystem<tauri::Wry>>>,
) -> Result<(), String> {
    avatar.head_pat();
    Ok(())
}

#[tauri::command]
pub async fn set_avatar_position(
    x: f64,
    y: f64,
    avatar: tauri::State<'_, std::sync::Arc<AvatarSystem<tauri::Wry>>>,
) -> Result<(), String> {
    avatar.set_position(x, y);
    Ok(())
}
