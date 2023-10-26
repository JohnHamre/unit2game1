use bytemuck::{Pod, Zeroable};
use kira::{
    manager::{backend::DefaultBackend, AudioManager, AudioManagerSettings},
    sound::static_sound::{StaticSoundData, StaticSoundSettings},
};
use rand::{thread_rng, Rng};
use std::borrow::Cow;
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::Window,
};
mod enemy_ai;
mod input;

// Sprite Sheet Resolution
const SPRITE_SHEET_RESOLUTION: (f32, f32) = (12.0, 16.0);

#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod)]
struct GPUCamera {
    screen_pos: [f32; 2],
    screen_size: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod, Debug, PartialEq)]
struct GPUSprite {
    screen_region: [f32; 4],
    sheet_region: [f32; 4],
}

struct TransitionFlag {
    val: usize
}

// A massive struct used to hold every major variable in the game.
struct GameStateHolder {
    player: Player,
    enemy: Entity,
    sprite_holder: SpriteHolder,
    projectiles: Vec<Projectile>,
    input: input::Input,
    player_health_bar: HealthBar,
    game_state: GameState,
    background: Screen,
    title_screen: Screen,
    death_screen: Screen,
    cleared_screen: Screen,
    win_screen: Screen,
    title_screen_2: Screen,
    sound_manager: AudioManager,
    trans_flag: TransitionFlag,
}

struct GameState {
    // This should be done better... but it isn't.
    /*
       0 = Title Screen
       1 = Gameplay
       2 = Game Over
       3 = Stage Cleared
       4 = You Win!
       5 = Title 2
       6 = Danmaku Game
       7 = Danmaku Game Death Screen
    */
    state: usize,
}

struct Screen {
    sprite: GPUSprite,
    sprite_index: usize,
}

#[derive(Debug, Clone)]
pub struct SpriteHolder {
    sprites: Vec<GPUSprite>,
    active: Vec<bool>,
}

impl SpriteHolder {
    // Gets the next free index for adding a new sprite.
    fn get_next_index(&mut self) -> usize {
        for i in 0..self.active.len() {
            // Optionals are great.
            match self.active.get(i) {
                Some(b) => {
                    if !b {
                        self.active[i] = true;
                        return i;
                    }
                }
                None => {}
            }
        }

        // This case will never happen but rust thinks it might.
        return 0;
    }

    // When an object dies, remove its sprite to prevent lingering graphics
    fn remove_sprite(&mut self, sprite_index: usize) {
        // Open up the sprite to be used by a future object.
        self.active[sprite_index] = false;
        // And disable rendering for the sprite (by zeroing all its values)
        self.sprites[sprite_index] = GPUSprite::zeroed();
    }

    fn set_sprite(&mut self, sprite_index: usize, sprite: GPUSprite) {
        // Flag the sprite as in use.
        self.active[sprite_index] = true;
        // Set the sprite data as passed.
        self.sprites[sprite_index] = sprite;
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Projectile {
    pos: (f32, f32),
    size: (f32, f32),
    speed: f32,
    velocity: (f32, f32),
    sprite_index: usize,
    sprite: GPUSprite,
    is_dead: bool,
    player_spawned: bool,
}

impl Projectile {
    // Called each frame to move the projectile
    fn move_proj(&mut self, player_health_bar: &mut HealthBar, sound_manager: &mut AudioManager, trans_flag: &mut TransitionFlag, game_state: usize) {
        // Move down by <speed> amount
        self.pos = (self.pos.0 + self.velocity.0, self.pos.1 + self.velocity.1);

        if self.pos.1 < 0.0 {
            self.kill();
            if game_state == 1 {
                let sound_data =
                StaticSoundData::from_file("src/content/projectile_missed.ogg", StaticSoundSettings::default())
                    .unwrap();
    
                let _ = sound_manager.play(sound_data);
                Player::damage(1.0, player_health_bar, trans_flag, 1);
            }
        }
        // Remove if too high
        else if self.pos.1 > 1000.0 {
            self.kill();
        }

        // Update sprite location.
        self.sprite.screen_region = [self.pos.0, self.pos.1, self.size.0, self.size.1];
    }

    fn check_collision(
        &mut self,
        player: &mut Player,
        enemy: &mut Enemy,
        sound_manager: &mut AudioManager,
        trans_flag: &mut TransitionFlag,
        player_health_bar: &mut HealthBar,
        game_state: usize,
    ) {
        if self.player_spawned {
            // Check for collision
            if self.pos.1 <= enemy.pos.1 + enemy.size.1
                && self.pos.1 + self.size.1 >= enemy.pos.1
                && self.pos.0 <= enemy.pos.0 + enemy.size.0
                && self.pos.0 + self.size.0 >= enemy.pos.0
            {
                let sound_data =
                    StaticSoundData::from_file("src/content/enemy_hit.ogg", StaticSoundSettings::default())
                        .unwrap();

                let _ = sound_manager.play(sound_data);

                // Handle logic.
                enemy.damage(1.0, trans_flag);
                // If colliding, remove projectile
                self.kill();
            }
        } else {
            // Check for collision
            if self.pos.1 <= player.pos.1 + player.size.1
                && self.pos.1 + self.size.1 >= player.pos.1
                && self.pos.0 <= player.pos.0 + player.size.0
                && self.pos.0 + self.size.0 >= player.pos.0
            {
                if game_state == 1 {
                    let sound_data =
                    StaticSoundData::from_file("src/content/player_hit.ogg", StaticSoundSettings::default())
                        .unwrap();

                    let _ = sound_manager.play(sound_data);
                    // Handle logic.
                    player.charges += 1;
                }
                if game_state == 6 {
                    Player::damage(1.0, player_health_bar, trans_flag, 6);
                }
                // If colliding, remove projectile
                self.kill();
            } else {
                // TODO: player missed
            }
        }
    }

    fn kill(&mut self) {
        self.is_dead = true;
    }

    fn clean_dead(&mut self, sprite_holder: &mut SpriteHolder) {
        sprite_holder.remove_sprite(self.sprite_index);
    }
}

pub struct Player {
    pos: (f32, f32),
    size: (f32, f32),
    speed: f32,
    velocity: (f32, f32),
    sprite_index: usize,
    facing_right: bool,
    sprite: GPUSprite,
    charges: usize,
}

impl Player {
    fn player_loop(&mut self, sprite_holder: &mut SpriteHolder) {
        if self.velocity.0 > 0.0 {
            self.pos = (self.pos.0 + self.speed, self.pos.1);
            if self.pos.0 > 960.0 {
                self.pos.0 = 960.0;
            }
            self.facing_right = true;
        }
        if self.velocity.0 < 0.0 {
            self.pos = (self.pos.0 - self.speed, self.pos.1);
            if self.pos.0 < 0.0 {
                self.pos.0 = 0.0;
            }
            self.facing_right = false;
        }

        self.sprite.screen_region = [self.pos.0, self.pos.1, self.size.0, self.size.1];

        if self.facing_right {
            set_sprite(&mut self.sprite, (0.0, 0.0))
        } else {
            set_sprite(&mut self.sprite, (2.0, 0.0))
        }

        // Sync sprite to Sprite Holder.
        sprite_holder.set_sprite(self.sprite_index, self.sprite);
    }

    fn damage(amount: f32, player_health_bar: &mut HealthBar, trans_flag: &mut TransitionFlag, game_state: usize) {
        player_health_bar.currval -= amount;
        if player_health_bar.currval <= 0.0 {
            if game_state == 1 {
                trans_flag.val = 2;
            }
            else if game_state == 6 {
                trans_flag.val = 7;
            }
        }
    }

    fn add_speed(&mut self, new_velocity: (f32, f32)) {
        self.velocity = (
            self.velocity.0 + new_velocity.0,
            self.velocity.1 + new_velocity.1,
        );
    }

    fn spawn_new_projectile(
        &mut self,
        speed: f32,
        projectiles: &mut Vec<Projectile>,
        sprite_holder: &mut SpriteHolder,
        sound_manager: &mut AudioManager,
    ) {
        // Shoot if player has enough juice. 3 Apples = 1 Orange, ofc.
        if self.charges >= 3 {
            let sound_data =
                StaticSoundData::from_file("src/content/player_shoot.ogg", StaticSoundSettings::default())
                    .unwrap();
            let _ = sound_manager.play(sound_data);
            // Set velocity based on a random angle.
            let velocity = (0.0, speed);
            let pos = (self.pos.0, self.pos.1 + self.size.1);
            make_player_projectile(projectiles, sprite_holder.get_next_index(), pos, velocity);

            // Reset juice.
            self.charges = 0;
        }
    }
}

// Speed would matter if the enemy were able to move, but it doesn't in our current levels.
#[allow(dead_code)]
pub struct Enemy {
    pos: (f32, f32),
    size: (f32, f32),
    speed: f32,
    velocity: (f32, f32),
    frame: f32,
    sprite_index: usize,
    sprite_index_eyes: usize,
    sprite: GPUSprite,
    sprite_eyes: GPUSprite,
    health_bar: HealthBar,
}

impl Enemy {
    fn spawn_new_projectile(
        &self,
        projectiles: &mut Vec<Projectile>,
        sprite_holder: &mut SpriteHolder,
        velocity: (f32, f32),
    ) {                 
        // let sound_data =
        // StaticSoundData::from_file("src/content/enemy_shoot.ogg", StaticSoundSettings::default())
        //     .unwrap();

        // sound_manager.play(sound_data);
        // Set velocity based on a random angle.
        let pos = (450.0 + thread_rng().gen_range(-20..=20) as f32, 650.0);
        make_projectile(projectiles, sprite_holder.get_next_index(), pos, velocity)
    }

    fn damage(&mut self, amount: f32, trans_flag: &mut TransitionFlag) {
        self.health_bar.currval -= amount;
        if self.health_bar.currval <= 0.0 {
            trans_flag.val = 4;
        }
    }
}

struct Entity {
    enemy: Enemy,
    ai: Box<dyn enemy_ai::AI>,
}

impl Entity {
    fn enemy_loop(&mut self, projectiles: &mut Vec<Projectile>, sprite_holder: &mut SpriteHolder) {
        self.enemy.pos = (
            self.enemy.pos.0 + self.enemy.velocity.0,
            self.enemy.pos.1 + self.enemy.velocity.1,
        );

        // Sync the base sprite to screen position.
        self.enemy.sprite.screen_region = [
            self.enemy.pos.0,
            self.enemy.pos.1,
            self.enemy.size.0,
            self.enemy.size.1,
        ];

        // Animate the spikes of the spikey boi.
        if (self.enemy.frame * 20.0) as usize % 20 == 0 {
            self.enemy.sprite.sheet_region = [
                1.0 / SPRITE_SHEET_RESOLUTION.0,
                1.0 / SPRITE_SHEET_RESOLUTION.1,
                1.0 / SPRITE_SHEET_RESOLUTION.0,
                1.0 / SPRITE_SHEET_RESOLUTION.1,
            ];
        } else if (self.enemy.frame * 20.0) as usize % 10 == 0 {
            self.enemy.sprite.sheet_region = [
                2.0 / SPRITE_SHEET_RESOLUTION.0,
                1.0 / SPRITE_SHEET_RESOLUTION.1,
                1.0 / SPRITE_SHEET_RESOLUTION.0,
                1.0 / SPRITE_SHEET_RESOLUTION.1,
            ];
        }

        // Sync the eyes sprite to the screen pos and animate bob.
        self.enemy.sprite_eyes.screen_region = [
            self.enemy.pos.0,
            self.enemy.pos.1 - 2.0 + 4.0 * self.enemy.frame.sin(),
            self.enemy.size.0,
            self.enemy.size.1,
        ];

        self.ai.ai_loop(projectiles, sprite_holder, &self.enemy);

        self.enemy.health_bar.bar_pos = (
            self.enemy.pos.0 - 32.0,
            self.enemy.pos.1 + 72.0,
            self.enemy.health_bar.bar_pos.2,
            self.enemy.health_bar.bar_pos.3,
        );

        self.enemy.frame += 0.05;

        sprite_holder.set_sprite(self.enemy.sprite_index, self.enemy.sprite);
        sprite_holder.set_sprite(self.enemy.sprite_index_eyes, self.enemy.sprite_eyes);

        self.enemy.health_bar.health_bar_loop(sprite_holder);
    }
}

struct HealthBar {
    currval: f32,
    maxval: f32,
    bar_pos: (f32, f32, f32, f32),
    units_per_pixel: f32,
    sprite_bar: GPUSprite,
    sprite_border: GPUSprite,
    sprite_index_bar: usize,
    sprite_index_border: usize,
}

impl HealthBar {
    fn health_bar_loop(&mut self, sprite_holder: &mut SpriteHolder) {
        // Prevent Health Bar Underflow
        if self.currval < 0.0 {
            self.currval = 0.0;
        }

        self.sprite_bar.screen_region = [
            self.bar_pos.0,
            self.bar_pos.1 + self.units_per_pixel,
            self.bar_pos.2 * (self.currval / self.maxval),
            self.bar_pos.3 - (2.0 * self.units_per_pixel),
        ];

        self.sprite_border.screen_region = [
            self.bar_pos.0,
            self.bar_pos.1,
            self.bar_pos.2,
            self.bar_pos.3,
        ];

        sprite_holder.set_sprite(self.sprite_index_bar, self.sprite_bar);
        sprite_holder.set_sprite(self.sprite_index_border, self.sprite_border);
    }
}

#[cfg(not(feature = "webgl"))]
const USE_STORAGE: bool = true;
#[cfg(feature = "webgl")]
const USE_STORAGE: bool = false;

async fn run(event_loop: EventLoop<()>, window: Window) {
    // Initial game state. This object controls the state of the game.
    let game_state = GameState { state: 0 };

    let size = window.inner_size();

    log::info!("Use storage? {:?}", USE_STORAGE);

    let instance = wgpu::Instance::default();

    let surface = unsafe { instance.create_surface(&window) }.unwrap();
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            force_fallback_adapter: false,
            // Request an adapter which can render to our surface
            compatible_surface: Some(&surface),
        })
        .await
        .expect("Failed to find an appropriate adapter");

    // Create the logical device and command queue
    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                label: None,
                features: wgpu::Features::empty(),
                limits: if USE_STORAGE {
                    wgpu::Limits::downlevel_defaults()
                } else {
                    wgpu::Limits::downlevel_webgl2_defaults()
                }
                .using_resolution(adapter.limits()),
            },
            None,
        )
        .await
        .expect("Failed to create device");

    if USE_STORAGE {
        let supports_storage_resources = adapter
            .get_downlevel_capabilities()
            .flags
            .contains(wgpu::DownlevelFlags::VERTEX_STORAGE)
            && device.limits().max_storage_buffers_per_shader_stage > 0;
        assert!(supports_storage_resources, "Storage buffers not supported");
    }
    // Load the shaders from disk
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shader.wgsl"))),
    });

    let texture_bind_group_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            // It needs the first entry for the texture and the second for the sampler.
            // This is like defining a type signature.
            entries: &[
                // The texture binding
                wgpu::BindGroupLayoutEntry {
                    // This matches the binding in the shader
                    binding: 0,
                    // Only available in the fragment shader
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    // It's a texture binding
                    ty: wgpu::BindingType::Texture {
                        // We can use it with float samplers
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        // It's being used as a 2D texture
                        view_dimension: wgpu::TextureViewDimension::D2,
                        // This is not a multisampled texture
                        multisampled: false,
                    },
                    count: None,
                },
                // The sampler binding
                wgpu::BindGroupLayoutEntry {
                    // This matches the binding in the shader
                    binding: 1,
                    // Only available in the fragment shader
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    // It's a sampler
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    // No count
                    count: None,
                },
            ],
        });
    // The camera binding
    let camera_layout_entry = wgpu::BindGroupLayoutEntry {
        // This matches the binding in the shader
        binding: 0,
        // Available in vertex shader
        visibility: wgpu::ShaderStages::VERTEX,
        // It's a buffer
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        // No count, not a buffer array binding
        count: None,
    };
    let sprite_bind_group_layout = if USE_STORAGE {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                camera_layout_entry,
                wgpu::BindGroupLayoutEntry {
                    // This matches the binding in the shader
                    binding: 1,
                    // Available in vertex shader
                    visibility: wgpu::ShaderStages::VERTEX,
                    // It's a buffer
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    // No count, not a buffer array binding
                    count: None,
                },
            ],
        })
    } else {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[camera_layout_entry],
        })
    };
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[&sprite_bind_group_layout, &texture_bind_group_layout],
        push_constant_ranges: &[],
    });

    let swapchain_capabilities = surface.get_capabilities(&adapter);
    let swapchain_format = swapchain_capabilities.formats[0];

    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: if USE_STORAGE {
                "vs_storage_main"
            } else {
                "vs_vbuf_main"
            },
            buffers: if USE_STORAGE {
                &[]
            } else {
                &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<GPUSprite>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: std::mem::size_of::<[f32; 4]>() as u64,
                            shader_location: 1,
                        },
                    ],
                }]
            },
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "fs_main",
            targets: &[Some(swapchain_format.into())],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
    });

    let mut config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: swapchain_format,
        width: size.width,
        height: size.height,
        present_mode: wgpu::PresentMode::AutoVsync,
        alpha_mode: swapchain_capabilities.alpha_modes[0],
        view_formats: vec![],
    };

    surface.configure(&device, &config);

    let (sprite_tex, _sprite_img) =
        load_texture("src/content/spritesheet.png", None, &device, &queue)
            .await
            .expect("Couldn't load spritesheet texture");
    let view_sprite = sprite_tex.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler_sprite = device.create_sampler(&wgpu::SamplerDescriptor::default());
    let texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &texture_bind_group_layout,
        entries: &[
            // One for the texture, one for the sampler
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view_sprite),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler_sprite),
            },
        ],
    });
    let camera = GPUCamera {
        screen_pos: [0.0, 0.0],
        screen_size: [1024.0, 768.0],
    };
    let buffer_camera = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: bytemuck::bytes_of(&camera).len() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut sprite_holder = SpriteHolder {
        sprites: vec![GPUSprite::zeroed(); 1000],
        active: vec![false; 1000],
    };
    let buffer_sprite = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: sprite_holder.sprites.len() as u64 * std::mem::size_of::<GPUSprite>() as u64,
        usage: if USE_STORAGE {
            wgpu::BufferUsages::STORAGE
        } else {
            wgpu::BufferUsages::VERTEX
        } | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let sprite_bind_group = if USE_STORAGE {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &sprite_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer_camera.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: buffer_sprite.as_entire_binding(),
                },
            ],
        })
    } else {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &sprite_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer_camera.as_entire_binding(),
            }],
        })
    };
    queue.write_buffer(&buffer_camera, 0, bytemuck::bytes_of(&camera));
    queue.write_buffer(
        &buffer_sprite,
        0,
        bytemuck::cast_slice(&sprite_holder.sprites),
    );

    let sound_manager =
        AudioManager::<DefaultBackend>::new(AudioManagerSettings::default()).unwrap();

    // No one should read this mess of a declaration.
    // Contains a bunch of initial data for starting the game.
    let mut gso = GameStateHolder {
        game_state: game_state,
        player: Player {
            pos: (400.0, 100.0),
            size: (64.0, 64.0),
            speed: 6.0,
            velocity: (0.0, 0.0),
            sprite_index: 0,
            facing_right: true,
            sprite: GPUSprite {
                screen_region: [32.0, 128.0, 64.0, 64.0],
                sheet_region: [
                    0.0 / SPRITE_SHEET_RESOLUTION.0,
                    0.0 / SPRITE_SHEET_RESOLUTION.1,
                    1.0 / SPRITE_SHEET_RESOLUTION.0,
                    1.0 / SPRITE_SHEET_RESOLUTION.1,
                ],
            },
            charges: 0,
        },
        enemy: Entity {
            enemy: Enemy {
                pos: (450.0, 650.0),
                size: (64.0, 64.0),
                speed: 6.0,
                velocity: (0.0, 0.0),
                sprite_index: 0,
                sprite_index_eyes: 0,
                frame: 0.0,
                sprite: GPUSprite {
                    screen_region: [32.0, 128.0, 64.0, 64.0],
                    sheet_region: [
                        1.0 / SPRITE_SHEET_RESOLUTION.0,
                        1.0 / SPRITE_SHEET_RESOLUTION.1,
                        1.0 / SPRITE_SHEET_RESOLUTION.0,
                        1.0 / SPRITE_SHEET_RESOLUTION.1,
                    ],
                },
                sprite_eyes: GPUSprite {
                    screen_region: [32.0, 128.0, 64.0, 64.0],
                    sheet_region: [
                        3.0 / SPRITE_SHEET_RESOLUTION.0,
                        1.0 / SPRITE_SHEET_RESOLUTION.1,
                        1.0 / SPRITE_SHEET_RESOLUTION.0,
                        1.0 / SPRITE_SHEET_RESOLUTION.1,
                    ],
                },
                health_bar: HealthBar {
                    currval: 10.0,
                    maxval: 10.0,
                    bar_pos: (32.0, 600.0, 128.0, 24.0),
                    units_per_pixel: 4.0,
                    sprite_border: GPUSprite {
                        screen_region: [32.0, 32.0, 128.0, 24.0],
                        sheet_region: [
                            0.0 / SPRITE_SHEET_RESOLUTION.0,
                            2.0 / SPRITE_SHEET_RESOLUTION.1,
                            2.0 / SPRITE_SHEET_RESOLUTION.0,
                            (6.0 / 16.0) / SPRITE_SHEET_RESOLUTION.1,
                        ],
                    },
                    sprite_index_border: 0,
                    sprite_bar: GPUSprite {
                        screen_region: [32.0, 36.0, 128.0, 16.0],
                        sheet_region: [
                            0.0 / SPRITE_SHEET_RESOLUTION.0,
                            (2.0 + (12.0 / 16.0)) / SPRITE_SHEET_RESOLUTION.1,
                            2.0 / SPRITE_SHEET_RESOLUTION.0,
                            (4.0 / 16.0) / SPRITE_SHEET_RESOLUTION.1,
                        ],
                    },
                    sprite_index_bar: 0,
                },
            },
            ai: Box::new(enemy_ai::Level0AI {}),
        },
        projectiles: vec![],
        input: input::Input::default(),
        player_health_bar: HealthBar {
            currval: 10.0,
            maxval: 10.0,
            bar_pos: (32.0, 32.0, 128.0, 24.0),
            units_per_pixel: 4.0,
            sprite_border: GPUSprite {
                screen_region: [32.0, 32.0, 128.0, 24.0],
                sheet_region: [
                    0.0 / SPRITE_SHEET_RESOLUTION.0,
                    2.0 / SPRITE_SHEET_RESOLUTION.1,
                    2.0 / SPRITE_SHEET_RESOLUTION.0,
                    (6.0 / 16.0) / SPRITE_SHEET_RESOLUTION.1,
                ],
            },
            sprite_index_border: 0,
            sprite_bar: GPUSprite {
                screen_region: [32.0, 36.0, 128.0, 16.0],
                sheet_region: [
                    0.0 / SPRITE_SHEET_RESOLUTION.0,
                    (2.0 + (7.0 / 16.0)) / SPRITE_SHEET_RESOLUTION.1,
                    2.0 / SPRITE_SHEET_RESOLUTION.0,
                    (4.0 / 16.0) / SPRITE_SHEET_RESOLUTION.1,
                ],
            },
            sprite_index_bar: 0,
        },
        background: Screen {
            sprite: GPUSprite {
                screen_region: [0.0, 0.0, 1024.0, 760.0],
                sheet_region: [
                    0.0 / SPRITE_SHEET_RESOLUTION.0,
                    8.0 / SPRITE_SHEET_RESOLUTION.1,
                    12.0 / SPRITE_SHEET_RESOLUTION.0,
                    8.0 / SPRITE_SHEET_RESOLUTION.1,
                ],
            },
            sprite_index: sprite_holder.get_next_index(),
        },
        title_screen: Screen {
            sprite: GPUSprite {
                screen_region: [160.0, 32.0, 720.0, 720.0],
                sheet_region: [
                    0.0 / SPRITE_SHEET_RESOLUTION.0,
                    4.0 / SPRITE_SHEET_RESOLUTION.1,
                    4.0 / SPRITE_SHEET_RESOLUTION.0,
                    4.0 / SPRITE_SHEET_RESOLUTION.1,
                ],
            },
            sprite_index: sprite_holder.get_next_index(),
        },
        death_screen: Screen {
            sprite: GPUSprite {
                screen_region: [160.0, 32.0, 720.0, 720.0],
                sheet_region: [
                    8.0 / SPRITE_SHEET_RESOLUTION.0,
                    4.0 / SPRITE_SHEET_RESOLUTION.1,
                    4.0 / SPRITE_SHEET_RESOLUTION.0,
                    4.0 / SPRITE_SHEET_RESOLUTION.1,
                ],
            },
            sprite_index: sprite_holder.get_next_index(),
        },
        win_screen: Screen {
            sprite: GPUSprite {
                screen_region: [160.0, 32.0, 720.0, 720.0],
                sheet_region: [
                    4.0 / SPRITE_SHEET_RESOLUTION.0,
                    0.0 / SPRITE_SHEET_RESOLUTION.1,
                    4.0 / SPRITE_SHEET_RESOLUTION.0,
                    4.0 / SPRITE_SHEET_RESOLUTION.1,
                ],
            },
            sprite_index: sprite_holder.get_next_index(),
        },
        title_screen_2: Screen {
            sprite: GPUSprite {
                screen_region: [160.0, 32.0, 720.0, 720.0],
                sheet_region: [
                    8.0 / SPRITE_SHEET_RESOLUTION.0,
                    0.0 / SPRITE_SHEET_RESOLUTION.1,
                    4.0 / SPRITE_SHEET_RESOLUTION.0,
                    4.0 / SPRITE_SHEET_RESOLUTION.1,
                ],
            },
            sprite_index: sprite_holder.get_next_index(),
        },
        cleared_screen: Screen {
            sprite: GPUSprite {
                screen_region: [160.0, 32.0, 720.0, 720.0],
                sheet_region: [
                    8.0 / SPRITE_SHEET_RESOLUTION.0,
                    4.0 / SPRITE_SHEET_RESOLUTION.1,
                    4.0 / SPRITE_SHEET_RESOLUTION.0,
                    4.0 / SPRITE_SHEET_RESOLUTION.1,
                ],
            },
            sprite_index: sprite_holder.get_next_index(),
        },
        sprite_holder: sprite_holder,
        sound_manager: sound_manager,
        trans_flag: TransitionFlag { val: 0 },
    };

    event_loop.run(move |event, _, control_flow| {
        //*control_flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent {
                event: WindowEvent::Resized(size),
                ..
            } => {
                // Reconfigure the surface with the new size
                config.width = size.width;
                config.height = size.height;
                surface.configure(&device, &config);
                // On macos the window needs to be redrawn manually after resizing
                window.request_redraw();
            }
            Event::RedrawRequested(_) => {
                // Control the event loop in each state
                match gso.game_state.state {
                    0 => {
                        title_screen_loop(&mut gso);
                    }
                    1 => {
                        main_event_loop(&mut gso);
                    }
                    2 => {
                        death_screen_loop(&mut gso, 1);
                    }
                    3 => {
                        cleared_screen_loop(&mut gso);
                    }
                    4 => {
                        win_screen_loop(&mut gso);
                    }
                    5 => {
                        title_screen_2_loop(&mut gso);
                    }
                    6 => {
                        main_event_loop(&mut gso);
                    }
                    7 => {
                        death_screen_loop(&mut gso, 6);
                    }
                    _ => {
                        println!("INVALID STATE {} REACHED!", gso.game_state.state);
                    }
                }

                // Then send the data to the GPU!
                gso.input.next_frame();
                queue.write_buffer(&buffer_camera, 0, bytemuck::bytes_of(&camera));
                queue.write_buffer(
                    &buffer_sprite,
                    0,
                    bytemuck::cast_slice(&gso.sprite_holder.sprites),
                );

                let frame = surface
                    .get_current_texture()
                    .expect("Failed to acquire next swap chain texture");
                let view = frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                let mut encoder =
                    device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
                {
                    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: None,
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                store: true,
                            },
                        })],
                        depth_stencil_attachment: None,
                    });
                    rpass.set_pipeline(&render_pipeline);
                    if !USE_STORAGE {
                        rpass.set_vertex_buffer(0, buffer_sprite.slice(..));
                    }
                    rpass.set_bind_group(0, &sprite_bind_group, &[]);
                    rpass.set_bind_group(1, &texture_bind_group, &[]);
                    // draw two triangles per sprite, and sprites-many sprites.
                    // this uses instanced drawing, but it would also be okay
                    // to draw 6 * sprites.len() vertices and use modular arithmetic
                    // to figure out which sprite we're drawing.
                    rpass.draw(0..6, 0..(gso.sprite_holder.sprites.len() as u32));
                }
                queue.submit(Some(encoder.finish()));
                frame.present();

                window.request_redraw();
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *control_flow = ControlFlow::Exit,
            // WindowEvent->KeyboardInput: Keyboard input!
            Event::WindowEvent {
                // Note this deeply nested pattern match
                event: WindowEvent::KeyboardInput { input: key_ev, .. },
                ..
            } => {
                gso.input.handle_key_event(key_ev);
            }
            Event::WindowEvent {
                event: WindowEvent::MouseInput { state, button, .. },
                ..
            } => {
                gso.input.handle_mouse_button(state, button);
            }
            Event::WindowEvent {
                event: WindowEvent::CursorMoved { position, .. },
                ..
            } => {
                gso.input.handle_mouse_move(position);
            }
            Event::MainEventsCleared => {
                window.request_redraw();
            }
            _ => {}
        }
    });
}

fn main() {
    let event_loop = EventLoop::new();
    let window = winit::window::Window::new(&event_loop).unwrap();
    #[cfg(not(target_arch = "wasm32"))]
    {
        env_logger::init();
        pollster::block_on(run(event_loop, window));
    }
    #[cfg(target_arch = "wasm32")]
    {
        std::panic::set_hook(Box::new(console_error_panic_hook::hook));
        console_log::init_with_level(log::Level::Trace).expect("could not initialize logger");
        use winit::platform::web::WindowExtWebSys;
        // On wasm, append the canvas to the document body
        web_sys::window()
            .and_then(|win| win.document())
            .and_then(|doc| doc.body())
            .and_then(|body| {
                body.append_child(&web_sys::Element::from(window.canvas()))
                    .ok()
            })
            .expect("couldn't append canvas to document body");
        wasm_bindgen_futures::spawn_local(run(event_loop, window));
    }
}
async fn load_texture(
    path: impl AsRef<std::path::Path>,
    label: Option<&str>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<(wgpu::Texture, image::RgbaImage), Box<dyn std::error::Error>> {
    #[cfg(target_arch = "wasm32")]
    let img = {
        let fetch = web_sys::window()
            .map(|win| win.fetch_with_str(path.as_ref().to_str().unwrap()))
            .unwrap();
        let resp: web_sys::Response = wasm_bindgen_futures::JsFuture::from(fetch)
            .await
            .unwrap()
            .into();
        log::debug!("{:?} {:?}", &resp, resp.status());
        let buf: js_sys::ArrayBuffer =
            wasm_bindgen_futures::JsFuture::from(resp.array_buffer().unwrap())
                .await
                .unwrap()
                .into();
        log::debug!("{:?} {:?}", &buf, buf.byte_length());
        let u8arr = js_sys::Uint8Array::new(&buf);
        log::debug!("{:?}, {:?}", &u8arr, u8arr.length());
        let mut bytes = vec![0; u8arr.length() as usize];
        log::debug!("{:?}", &bytes);
        u8arr.copy_to(&mut bytes);
        image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
            .map_err(|e| e.to_string())?
            .to_rgba8()
    };
    #[cfg(not(target_arch = "wasm32"))]
    let img = image::open(path.as_ref())?.to_rgba8();
    let (width, height) = img.dimensions();
    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label,
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        texture.as_image_copy(),
        &img,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(4 * width),
            rows_per_image: Some(height),
        },
        size,
    );
    Ok((texture, img))
}

fn set_sprite(sprite: &mut GPUSprite, index: (f32, f32)) {
    sprite.sheet_region = [
        index.0 / SPRITE_SHEET_RESOLUTION.0,
        index.1 / SPRITE_SHEET_RESOLUTION.1,
        1.0 / SPRITE_SHEET_RESOLUTION.0,
        1.0 / SPRITE_SHEET_RESOLUTION.1,
    ];
}

fn make_projectile(
    projectiles: &mut Vec<Projectile>,
    index: usize,
    spawn_pos: (f32, f32),
    velocity: (f32, f32),
) {
    let projectile = Projectile {
        pos: (spawn_pos.0, spawn_pos.1),
        size: (64.0, 64.0),
        speed: 10.0,
        velocity: (velocity.0, velocity.1),
        sprite_index: index,
        sprite: GPUSprite {
            screen_region: [2.0, 32.0, 64.0, 64.0],
            sheet_region: [
                0.0 / SPRITE_SHEET_RESOLUTION.0,
                1.0 / SPRITE_SHEET_RESOLUTION.1,
                1.0 / SPRITE_SHEET_RESOLUTION.0,
                1.0 / SPRITE_SHEET_RESOLUTION.1,
            ],
        },
        is_dead: false,
        player_spawned: false,
    };
    projectiles.push(projectile);
}

fn make_player_projectile(
    projectiles: &mut Vec<Projectile>,
    index: usize,
    spawn_pos: (f32, f32),
    velocity: (f32, f32),
) {
    let projectile = Projectile {
        pos: (spawn_pos.0, spawn_pos.1),
        size: (64.0, 64.0),
        speed: 10.0,
        velocity: (velocity.0, velocity.1),
        sprite_index: index,
        sprite: GPUSprite {
            screen_region: [2.0, 32.0, 64.0, 64.0],
            sheet_region: [
                3.0 / SPRITE_SHEET_RESOLUTION.0,
                2.0 / SPRITE_SHEET_RESOLUTION.1,
                1.0 / SPRITE_SHEET_RESOLUTION.0,
                1.0 / SPRITE_SHEET_RESOLUTION.1,
            ],
        },
        is_dead: false,
        player_spawned: true,
    };
    projectiles.push(projectile);
}

fn main_event_loop(gso: &mut GameStateHolder) {
    // Player movement!
    if gso
        .input
        .is_key_pressed(winit::event::VirtualKeyCode::Right)
    {
        gso.player.add_speed((gso.player.speed, 0.0))
    }
    if gso.input.is_key_pressed(winit::event::VirtualKeyCode::Left) {
        gso.player.add_speed((-gso.player.speed, 0.0))
    }
    if gso
        .input
        .is_key_released(winit::event::VirtualKeyCode::Right)
    {
        gso.player.add_speed((-gso.player.speed, 0.0))
    }
    if gso
        .input
        .is_key_released(winit::event::VirtualKeyCode::Left)
    {
        gso.player.add_speed((gso.player.speed, 0.0))
    }

    gso.sprite_holder.set_sprite(gso.background.sprite_index, gso.background.sprite);

    // Shoot!
    if gso.input.is_key_down(winit::event::VirtualKeyCode::Space) {
        gso.player.spawn_new_projectile(
            10.0,
            &mut gso.projectiles,
            &mut gso.sprite_holder,
            &mut gso.sound_manager,
        )
    }

    // Loop for the player
    gso.player.player_loop(&mut gso.sprite_holder);

    gso.player_health_bar
        .health_bar_loop(&mut gso.sprite_holder);

    if gso.game_state.state == 6 {
        gso.enemy.enemy.damage(1.0, &mut gso.trans_flag);
    }
    
    // Loop for the enemy
    gso.enemy
        .enemy_loop(&mut gso.projectiles, &mut gso.sprite_holder);

    // Move projectile
    for proj in gso.projectiles.iter_mut() {
        proj.move_proj(&mut gso.player_health_bar, &mut gso.sound_manager, &mut gso.trans_flag, gso.game_state.state);
        proj.check_collision(
            &mut gso.player,
            &mut gso.enemy.enemy,
            &mut gso.sound_manager,
            &mut gso.trans_flag,
            &mut gso.player_health_bar,
            gso.game_state.state,
        );
        gso.sprite_holder.set_sprite(proj.sprite_index, proj.sprite);
    }
    // Code to remove projectiles. Not very optimal but rust likes it.
    gso.projectiles.iter_mut().for_each(|proj| {
        if proj.is_dead {
            proj.clean_dead(&mut gso.sprite_holder)
        }
    });
    gso.projectiles.retain(|proj| !proj.is_dead);

    // Watch for updating gamestate
    if gso.trans_flag.val != 0 {
        transition_to_state(gso.trans_flag.val, gso);
    }
}

fn title_screen_loop(gso: &mut GameStateHolder) {
    if gso.input.is_key_down(winit::event::VirtualKeyCode::Space) {
        transition_to_state(1, gso);
        gso.title_screen.sprite.screen_region = [0.0, 0.0, 0.0, 0.0];
        gso.sprite_holder.set_sprite(gso.title_screen.sprite_index, gso.title_screen.sprite);
    }
    else if gso.input.is_key_down(winit::event::VirtualKeyCode::Right) {
        transition_to_state(5, gso);
        gso.title_screen.sprite.screen_region = [0.0, 0.0, 0.0, 0.0];
        gso.sprite_holder.set_sprite(gso.title_screen.sprite_index, gso.title_screen.sprite);
    }

    gso.sprite_holder
        .set_sprite(gso.title_screen.sprite_index, gso.title_screen.sprite);
}

fn death_screen_loop (gso: &mut GameStateHolder, next_state: usize) {
    if gso.input.is_key_down(winit::event::VirtualKeyCode::Space) {
        transition_to_state(next_state, gso);
        gso.death_screen.sprite.screen_region = [0.0, 0.0, 0.0, 0.0];
        gso.sprite_holder.set_sprite(gso.death_screen.sprite_index, gso.death_screen.sprite);
    }

    gso.sprite_holder.set_sprite(gso.death_screen.sprite_index, gso.death_screen.sprite);
}

fn cleared_screen_loop (gso: &mut GameStateHolder) {
    if gso.input.is_key_down(winit::event::VirtualKeyCode::Space) {
        transition_to_state(1, gso);
        gso.cleared_screen.sprite.screen_region = [0.0, 0.0, 0.0, 0.0];
        gso.sprite_holder.set_sprite(gso.cleared_screen.sprite_index, gso.cleared_screen.sprite);
    }

    gso.sprite_holder.set_sprite(gso.cleared_screen.sprite_index, gso.cleared_screen.sprite);
}

fn win_screen_loop (gso: &mut GameStateHolder) {
    gso.sprite_holder.set_sprite(gso.win_screen.sprite_index, gso.win_screen.sprite);
}

fn title_screen_2_loop (gso: &mut GameStateHolder) {
    if gso.input.is_key_down(winit::event::VirtualKeyCode::Space) {
        transition_to_state(6, gso);
        gso.title_screen_2.sprite.screen_region = [0.0, 0.0, 0.0, 0.0];
        gso.sprite_holder.set_sprite(gso.title_screen_2.sprite_index, gso.title_screen_2.sprite);
    }
    else if gso.input.is_key_down(winit::event::VirtualKeyCode::Left) {
        transition_to_state(0, gso);
        gso.title_screen_2.sprite.screen_region = [0.0, 0.0, 0.0, 0.0];
        gso.sprite_holder.set_sprite(gso.title_screen_2.sprite_index, gso.title_screen_2.sprite);
    }

    gso.sprite_holder
        .set_sprite(gso.title_screen_2.sprite_index, gso.title_screen_2.sprite);
}


fn transition_to_state(new_state: usize, gso: &mut GameStateHolder) {
    match gso.game_state.state{
        0 => {
            match new_state {
                1 => {
                    gso.game_state.state = new_state;
                    load_level_1(gso);
                }
                5 => {
                    gso.game_state.state = new_state;
                    gso.title_screen_2.sprite.screen_region = [160.0, 32.0, 720.0, 720.0];
                    gso.game_state.state = new_state;
                }
                _ => {
                    println!("Cannot transition from state {} to state {}", gso.game_state.state, new_state);
                }
            }
        }
        1 => {
            // Reset Transition Flag
            gso.trans_flag.val = 0;
            match new_state {
                // Game Over
                2 => {
                    gso.death_screen.sprite.screen_region = [160.0, 32.0, 720.0, 720.0];
                    gso.game_state.state = new_state;
                    load_dead_level(gso);
                }
                // Stage Cleared
                3 => {
                    gso.cleared_screen.sprite.screen_region = [160.0, 32.0, 720.0, 720.0];
                    gso.game_state.state = new_state;
                    load_dead_level(gso);
                }
                // You  Win
                4 => {
                    gso.win_screen.sprite.screen_region = [160.0, 32.0, 720.0, 720.0];
                    gso.game_state.state = new_state;
                    load_dead_level(gso);
                }
                _ => {
                    println!("Cannot transition from state {} to state {}", gso.game_state.state, new_state);
                }
            }
        }
        2 => {
            match new_state {
                1 => {
                    gso.game_state.state = new_state;
                    load_level_1(gso);
                }
                _ => {
                    println!("Cannot transition from state {} to state {}", gso.game_state.state, new_state);
                }
            }
        }
        3 => {
            match new_state {
                1 => {
                    gso.game_state.state = new_state;
                    load_level_1(gso);
                }
                _ => {
                    println!("Cannot transition from state {} to state {}", gso.game_state.state, new_state);
                }
            }
        }
        5 => {
            match new_state {
                6 => {
                    gso.game_state.state = new_state;
                    load_level_6(gso);
                }
                0 => {
                    gso.game_state.state = new_state;
                    gso.title_screen.sprite.screen_region = [160.0, 32.0, 720.0, 720.0];
                    gso.game_state.state = new_state;
                }
                _ => {
                    println!("Cannot transition from state {} to state {}", gso.game_state.state, new_state);
                }
            }
        }
        6 => {
            // Reset Transition Flag
            gso.trans_flag.val = 0;
            match new_state {
                // Game Over
                7 => {
                    gso.death_screen.sprite.screen_region = [160.0, 32.0, 720.0, 720.0];
                    gso.game_state.state = new_state;
                    load_dead_level(gso);
                }
                // Stage Cleared
                3 => {
                    gso.cleared_screen.sprite.screen_region = [160.0, 32.0, 720.0, 720.0];
                    gso.game_state.state = new_state;
                    load_dead_level(gso);
                }
                // You  Win
                4 => {
                    gso.win_screen.sprite.screen_region = [160.0, 32.0, 720.0, 720.0];
                    gso.game_state.state = new_state;
                    load_dead_level(gso);
                }
                _ => {
                    println!("Cannot transition from state {} to state {}", gso.game_state.state, new_state);
                }
            }
        }
        7 => {
            match new_state {
                6 => {
                    gso.game_state.state = new_state;
                    load_level_6(gso);
                }
                _ => {
                    println!("Cannot transition from state {} to state {}", gso.game_state.state, new_state);
                }
            }
        }
        _ => {
            println!("Cannot transition from state {}", gso.game_state.state);
        }
    }
}

fn load_dead_level(gso : &mut GameStateHolder) {
    // Clear out old sprites.
    gso.sprite_holder.remove_sprite(gso.player.sprite_index);
    gso.sprite_holder.remove_sprite(gso.enemy.enemy.sprite_index);
    gso.sprite_holder.remove_sprite(gso.enemy.enemy.sprite_index_eyes);
    gso.sprite_holder.remove_sprite(gso.enemy.enemy.health_bar.sprite_index_bar);
    gso.sprite_holder.remove_sprite(gso.enemy.enemy.health_bar.sprite_index_border);
    gso.sprite_holder.remove_sprite(gso.player_health_bar.sprite_index_bar);
    gso.sprite_holder.remove_sprite(gso.player_health_bar.sprite_index_border);

    // Purge Projectiles
    gso.projectiles.iter_mut().for_each(|proj| {proj.kill(); if proj.is_dead {proj.clean_dead(&mut gso.sprite_holder)}});
    gso.projectiles.retain(|proj| !proj.is_dead);

    // Set values to dead state values.
    gso.player = Player {
        pos: (400.0, 100.0),
        size: (64.0, 64.0),
        speed: 6.0,
        velocity: (0.0, 0.0),
        sprite_index: 0,
        facing_right: true,
        sprite: GPUSprite {
            screen_region: [32.0, 128.0, 64.0, 64.0],
            sheet_region: [0.0 / SPRITE_SHEET_RESOLUTION.0, 0.0 / SPRITE_SHEET_RESOLUTION.1, 1.0 / SPRITE_SHEET_RESOLUTION.0, 1.0 / SPRITE_SHEET_RESOLUTION.1],
        },
        charges: 0,
    };
    gso.enemy = Entity {
        enemy: Enemy {
            pos: (450.0, 650.0),
            size: (64.0, 64.0),
            speed: 6.0,
            velocity: (0.0, 0.0),
            sprite_index: 0,
            sprite_index_eyes: 0,
            frame: 0.0,
            sprite: GPUSprite {
                screen_region: [32.0, 128.0, 64.0, 64.0],
                sheet_region: [1.0 / SPRITE_SHEET_RESOLUTION.0, 1.0 / SPRITE_SHEET_RESOLUTION.1, 1.0 / SPRITE_SHEET_RESOLUTION.0, 1.0 / SPRITE_SHEET_RESOLUTION.1],
            },
            sprite_eyes: GPUSprite {
                screen_region: [32.0, 128.0, 64.0, 64.0],
                sheet_region: [3.0 / SPRITE_SHEET_RESOLUTION.0, 1.0 / SPRITE_SHEET_RESOLUTION.1, 1.0 / SPRITE_SHEET_RESOLUTION.0, 1.0 / SPRITE_SHEET_RESOLUTION.1],
            },
            health_bar: HealthBar {
                currval: 10.0,
                maxval: 10.0,
                bar_pos: (32.0, 600.0, 128.0, 24.0),
                units_per_pixel: 4.0,
                sprite_border: GPUSprite {
                    screen_region: [32.0, 32.0, 128.0, 24.0],
                    sheet_region: [0.0 / SPRITE_SHEET_RESOLUTION.0, 2.0 / SPRITE_SHEET_RESOLUTION.1, 2.0 / SPRITE_SHEET_RESOLUTION.0, (6.0 / 16.0) / SPRITE_SHEET_RESOLUTION.1],
                },
                sprite_index_border: 0,
                sprite_bar: GPUSprite {
                    screen_region: [32.0, 36.0, 128.0, 16.0],
                    sheet_region: [0.0 / SPRITE_SHEET_RESOLUTION.0, (2.0  + (12.0 / 16.0)) / SPRITE_SHEET_RESOLUTION.1, 2.0 / SPRITE_SHEET_RESOLUTION.0, (4.0 / 16.0) / SPRITE_SHEET_RESOLUTION.1],
                },
                sprite_index_bar: 0,
            },
        },
        ai: Box::new(enemy_ai::Level0AI {})
    };
    gso.player_health_bar = HealthBar {
        currval: 10.0,
        maxval: 10.0,
        bar_pos: (32.0, 32.0, 128.0, 24.0),
        units_per_pixel: 4.0,
        sprite_border: GPUSprite {
            screen_region: [32.0, 32.0, 128.0, 24.0],
            sheet_region: [0.0 / SPRITE_SHEET_RESOLUTION.0, 2.0 / SPRITE_SHEET_RESOLUTION.1, 2.0 / SPRITE_SHEET_RESOLUTION.0, (6.0 / 16.0) / SPRITE_SHEET_RESOLUTION.1],
        },
        sprite_index_border: 0,
        sprite_bar: GPUSprite {
            screen_region: [32.0, 36.0, 128.0, 16.0],
            sheet_region: [0.0 / SPRITE_SHEET_RESOLUTION.0, (2.0  + (7.0 / 16.0)) / SPRITE_SHEET_RESOLUTION.1, 2.0 / SPRITE_SHEET_RESOLUTION.0, (4.0 / 16.0) / SPRITE_SHEET_RESOLUTION.1],
        },
        sprite_index_bar: 0,
    };
}

fn load_level_1(gso : &mut GameStateHolder) {
    gso.player = Player {
            pos: (400.0, 100.0),
            size: (64.0, 64.0),
            speed: 6.0,
            velocity: (0.0, 0.0),
            sprite_index: gso.sprite_holder.get_next_index(),
            facing_right: true,
            sprite: GPUSprite {
                screen_region: [32.0, 128.0, 64.0, 64.0],
                sheet_region: [0.0 / SPRITE_SHEET_RESOLUTION.0, 0.0 / SPRITE_SHEET_RESOLUTION.1, 1.0 / SPRITE_SHEET_RESOLUTION.0, 1.0 / SPRITE_SHEET_RESOLUTION.1],
            },
            charges: 0,
        };
    gso.enemy = Entity {
            enemy: Enemy {
                pos: (450.0, 650.0),
                size: (64.0, 64.0),
                speed: 6.0,
                velocity: (0.0, 0.0),
                sprite_index: gso.sprite_holder.get_next_index(),
                sprite_index_eyes: gso.sprite_holder.get_next_index(),
                frame: 0.0,
                sprite: GPUSprite {
                    screen_region: [32.0, 128.0, 64.0, 64.0],
                    sheet_region: [1.0 / SPRITE_SHEET_RESOLUTION.0, 1.0 / SPRITE_SHEET_RESOLUTION.1, 1.0 / SPRITE_SHEET_RESOLUTION.0, 1.0 / SPRITE_SHEET_RESOLUTION.1],
                },
                sprite_eyes: GPUSprite {
                    screen_region: [32.0, 128.0, 64.0, 64.0],
                    sheet_region: [3.0 / SPRITE_SHEET_RESOLUTION.0, 1.0 / SPRITE_SHEET_RESOLUTION.1, 1.0 / SPRITE_SHEET_RESOLUTION.0, 1.0 / SPRITE_SHEET_RESOLUTION.1],
                },
                health_bar: HealthBar {
                    currval: 10.0,
                    maxval: 10.0,
                    bar_pos: (32.0, 600.0, 128.0, 24.0),
                    units_per_pixel: 4.0,
                    sprite_border: GPUSprite {
                        screen_region: [32.0, 32.0, 128.0, 24.0],
                        sheet_region: [0.0 / SPRITE_SHEET_RESOLUTION.0, 2.0 / SPRITE_SHEET_RESOLUTION.1, 2.0 / SPRITE_SHEET_RESOLUTION.0, (6.0 / 16.0) / SPRITE_SHEET_RESOLUTION.1],
                    },
                    sprite_index_border: gso.sprite_holder.get_next_index(),
                    sprite_bar: GPUSprite {
                        screen_region: [32.0, 36.0, 128.0, 16.0],
                        sheet_region: [0.0 / SPRITE_SHEET_RESOLUTION.0, (2.0  + (12.0 / 16.0)) / SPRITE_SHEET_RESOLUTION.1, 2.0 / SPRITE_SHEET_RESOLUTION.0, (4.0 / 16.0) / SPRITE_SHEET_RESOLUTION.1],
                    },
                    sprite_index_bar: gso.sprite_holder.get_next_index(),
                },
            },
            ai: Box::new(enemy_ai::Level1AI {
                max_cooldown: 40,
                cooldown: 0,
            }),
        };
    gso.player_health_bar = HealthBar {
        currval: 10.0,
        maxval: 10.0,
        bar_pos: (32.0, 32.0, 128.0, 24.0),
        units_per_pixel: 4.0,
        sprite_border: GPUSprite {
            screen_region: [32.0, 32.0, 128.0, 24.0],
            sheet_region: [0.0 / SPRITE_SHEET_RESOLUTION.0, 2.0 / SPRITE_SHEET_RESOLUTION.1, 2.0 / SPRITE_SHEET_RESOLUTION.0, (6.0 / 16.0) / SPRITE_SHEET_RESOLUTION.1],
        },
        sprite_index_border: gso.sprite_holder.get_next_index(),
        sprite_bar: GPUSprite {
            screen_region: [32.0, 36.0, 128.0, 16.0],
            sheet_region: [0.0 / SPRITE_SHEET_RESOLUTION.0, (2.0  + (7.0 / 16.0)) / SPRITE_SHEET_RESOLUTION.1, 2.0 / SPRITE_SHEET_RESOLUTION.0, (4.0 / 16.0) / SPRITE_SHEET_RESOLUTION.1],
        },
        sprite_index_bar: gso.sprite_holder.get_next_index(),
    }
}

fn load_level_6(gso : &mut GameStateHolder) {
    gso.player = Player {
            pos: (400.0, 100.0),
            size: (64.0, 64.0),
            speed: 6.0,
            velocity: (0.0, 0.0),
            sprite_index: gso.sprite_holder.get_next_index(),
            facing_right: true,
            sprite: GPUSprite {
                screen_region: [32.0, 128.0, 64.0, 64.0],
                sheet_region: [0.0 / SPRITE_SHEET_RESOLUTION.0, 0.0 / SPRITE_SHEET_RESOLUTION.1, 1.0 / SPRITE_SHEET_RESOLUTION.0, 1.0 / SPRITE_SHEET_RESOLUTION.1],
            },
            charges: 0,
        };
    gso.enemy = Entity {
            enemy: Enemy {
                pos: (450.0, 650.0),
                size: (64.0, 64.0),
                speed: 6.0,
                velocity: (0.0, 0.0),
                sprite_index: gso.sprite_holder.get_next_index(),
                sprite_index_eyes: gso.sprite_holder.get_next_index(),
                frame: 0.0,
                sprite: GPUSprite {
                    screen_region: [32.0, 128.0, 64.0, 64.0],
                    sheet_region: [1.0 / SPRITE_SHEET_RESOLUTION.0, 1.0 / SPRITE_SHEET_RESOLUTION.1, 1.0 / SPRITE_SHEET_RESOLUTION.0, 1.0 / SPRITE_SHEET_RESOLUTION.1],
                },
                sprite_eyes: GPUSprite {
                    screen_region: [32.0, 128.0, 64.0, 64.0],
                    sheet_region: [3.0 / SPRITE_SHEET_RESOLUTION.0, 1.0 / SPRITE_SHEET_RESOLUTION.1, 1.0 / SPRITE_SHEET_RESOLUTION.0, 1.0 / SPRITE_SHEET_RESOLUTION.1],
                },
                health_bar: HealthBar {
                    currval: 1800.0,
                    maxval: 1800.0,
                    bar_pos: (32.0, 600.0, 128.0, 24.0),
                    units_per_pixel: 4.0,
                    sprite_border: GPUSprite {
                        screen_region: [32.0, 32.0, 128.0, 24.0],
                        sheet_region: [0.0 / SPRITE_SHEET_RESOLUTION.0, 2.0 / SPRITE_SHEET_RESOLUTION.1, 2.0 / SPRITE_SHEET_RESOLUTION.0, (6.0 / 16.0) / SPRITE_SHEET_RESOLUTION.1],
                    },
                    sprite_index_border: gso.sprite_holder.get_next_index(),
                    sprite_bar: GPUSprite {
                        screen_region: [32.0, 36.0, 128.0, 16.0],
                        sheet_region: [0.0 / SPRITE_SHEET_RESOLUTION.0, (2.0  + (12.0 / 16.0)) / SPRITE_SHEET_RESOLUTION.1, 2.0 / SPRITE_SHEET_RESOLUTION.0, (4.0 / 16.0) / SPRITE_SHEET_RESOLUTION.1],
                    },
                    sprite_index_bar: gso.sprite_holder.get_next_index(),
                },
            },
            ai: Box::new(enemy_ai::Level6AI {
                max_cooldown: 40,
                cooldown: 0,
            }),
        };
    gso.player_health_bar = HealthBar {
        currval: 1.0,
        maxval: 1.0,
        bar_pos: (32.0, 32.0, 128.0, 24.0),
        units_per_pixel: 4.0,
        sprite_border: GPUSprite {
            screen_region: [32.0, 32.0, 128.0, 24.0],
            sheet_region: [0.0 / SPRITE_SHEET_RESOLUTION.0, 2.0 / SPRITE_SHEET_RESOLUTION.1, 2.0 / SPRITE_SHEET_RESOLUTION.0, (6.0 / 16.0) / SPRITE_SHEET_RESOLUTION.1],
        },
        sprite_index_border: gso.sprite_holder.get_next_index(),
        sprite_bar: GPUSprite {
            screen_region: [32.0, 36.0, 128.0, 16.0],
            sheet_region: [0.0 / SPRITE_SHEET_RESOLUTION.0, (2.0  + (7.0 / 16.0)) / SPRITE_SHEET_RESOLUTION.1, 2.0 / SPRITE_SHEET_RESOLUTION.0, (4.0 / 16.0) / SPRITE_SHEET_RESOLUTION.1],
        },
        sprite_index_bar: gso.sprite_holder.get_next_index(),
    }
}