use bytemuck::{Pod, Zeroable};
use std::borrow::Cow;
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::Window,
};
use rand::{thread_rng, Rng};
mod input;

// Sprite Sheet Resolution
const SPRITE_SHEET_RESOLUTION: (f32, f32) = (4.0, 4.0);

#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod)]
struct GPUCamera {
    screen_pos: [f32; 2],
    screen_size: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod)]
struct GPUSprite {
    screen_region: [f32; 4],
    sheet_region: [f32; 4],
}

struct Projectile {
    pos: (f32, f32),
    size: (f32, f32),
    speed: f32,
    spawn_pos: (f32, f32),
    sprite_index: usize,
}

impl Projectile {
    // Called each frame to move the projectile
    fn move_proj(&mut self, sprites: &mut Vec<GPUSprite>) {
        // Move down by <speed> amount
        self.pos = (self.pos.0, self.pos.1 - self.speed);

        if self.pos.1 < 0.0 {
            self.respawn();
        }

        // Update sprite location.
        sprites[self.sprite_index].screen_region = 
        [
            self.pos.0,
            self.pos.1,
            self.size.0,
            self.size.1
        ];
    }

    fn check_collision (&mut self, player: &mut Player) {
        // Check for collision
        if self.pos.1 <= player.pos.1 + player.size.1 &&
           self.pos.1 + self.size.1 >= player.pos.1 &&
           self.pos.0 <= player.pos.0 + player.size.0 &&
           self.pos.0 + self.size.0 >= player.pos.0 
        {
            // If colliding, reset spawn and print
            self.respawn();
        }
    }

    fn respawn(&mut self) {
        self.pos = (self.spawn_pos.0 + thread_rng().gen_range(-200..=200) as f32, self.spawn_pos.1);
    }
}

struct Player {
    pos: (f32, f32),
    size: (f32, f32),
    speed: f32,
    sprite_index: usize,
    facing_right: bool,
}

impl Player {
    fn player_loop(&mut self, sprites: &mut Vec<GPUSprite>) {
        sprites[self.sprite_index].screen_region = 
        [
            self.pos.0,
            self.pos.1,
            self.size.0,
            self.size.1
        ];

        if self.facing_right {
            self.set_sprite(sprites, (0.0, 0.0))
        }
        else {
            self.set_sprite(sprites, (2.0, 0.0))
        }
    }

    fn set_sprite(&mut self, sprites: &mut Vec<GPUSprite>, index: (f32, f32)) {
        sprites[self.sprite_index].sheet_region = [
            index.0 / SPRITE_SHEET_RESOLUTION.0, 
            index.1 / SPRITE_SHEET_RESOLUTION.1, 
            1.0 / SPRITE_SHEET_RESOLUTION.0, 
            1.0 / SPRITE_SHEET_RESOLUTION.1
        ];
    }
}

#[cfg(not(feature = "webgl"))]
const USE_STORAGE: bool = true;
#[cfg(feature = "webgl")]
const USE_STORAGE: bool = false;

async fn run(event_loop: EventLoop<()>, window: Window) {
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

    let (sprite_tex, _sprite_img) = load_texture("content/spritesheet.png", None, &device, &queue)
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
    let mut sprites: Vec<GPUSprite> = vec![
        GPUSprite {
            screen_region: [2.0, 32.0, 64.0, 64.0],
            sheet_region: [0.0 / SPRITE_SHEET_RESOLUTION.0, 0.0 / SPRITE_SHEET_RESOLUTION.1, 1.0 / SPRITE_SHEET_RESOLUTION.0, 1.0 / SPRITE_SHEET_RESOLUTION.1],
        },
        GPUSprite {
            screen_region: [32.0, 128.0, 64.0, 64.0],
            sheet_region: [0.0 / SPRITE_SHEET_RESOLUTION.0, 1.0 / SPRITE_SHEET_RESOLUTION.1, 1.0 / SPRITE_SHEET_RESOLUTION.0, 1.0 / SPRITE_SHEET_RESOLUTION.1],
        },
        // GPUSprite {
        //     screen_region: [-128.0, 32.0, 64.0, 64.0],
        //     sheet_region: [0.0, 16.0 / 32.0, 16.0 / 32.0, 16.0 / 32.0],
        // },
        // GPUSprite {
        //     screen_region: [-128.0, 128.0, 64.0, 64.0],
        //     sheet_region: [16.0 / 32.0, 16.0 / 32.0, 16.0 / 32.0, 16.0 / 32.0],
        // },
    ];
    let buffer_sprite = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: sprites.len() as u64 * std::mem::size_of::<GPUSprite>() as u64,
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
    queue.write_buffer(&buffer_sprite, 0, bytemuck::cast_slice(&sprites));
    let mut input = input::Input::default();
    // Input bools
    let mut move_right = false;
    let mut move_left = false;
    // Define our projectile
    let mut projectile = Projectile{
        pos: (400.0, 500.0),
        size: (64.0, 64.0),
        speed: 10.0,
        spawn_pos: (400.0, 800.0),
        sprite_index: 1,
    };

    // And our player
    let mut player = Player {
        pos: (400.0, 100.0),
        size: (64.0, 64.0),
        speed: 6.0,
        sprite_index: 0,
        facing_right: true,
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
                // TODO: move sprites, maybe scroll camera
                if input.is_key_pressed(winit::event::VirtualKeyCode::Right) {
                    move_right = true;
                }
                if input.is_key_down(winit::event::VirtualKeyCode::Left) {
                    move_left = true;
                }
                if input.is_key_released(winit::event::VirtualKeyCode::Right) {
                    move_right = false;
                }
                if input.is_key_released(winit::event::VirtualKeyCode::Left) {
                    move_left = false;
                }

                if move_right {
                    player.pos = (player.pos.0 + player.speed, player.pos.1);
                    player.facing_right = true;
                }
                if move_left {
                    player.pos = (player.pos.0 - player.speed, player.pos.1);
                    player.facing_right = false;
                }

                player.player_loop(&mut sprites);

                // Move projectile
                projectile.move_proj(&mut sprites);

                projectile.check_collision(&mut player);

                // Then send the data to the GPU!
                input.next_frame();
                queue.write_buffer(&buffer_camera, 0, bytemuck::bytes_of(&camera));
                queue.write_buffer(&buffer_sprite, 0, bytemuck::cast_slice(&sprites));

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
                    rpass.draw(0..6, 0..(sprites.len() as u32));
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
                input.handle_key_event(key_ev);
            }
            Event::WindowEvent {
                event: WindowEvent::MouseInput { state, button, .. },
                ..
            } => {
                input.handle_mouse_button(state, button);
            }
            Event::WindowEvent {
                event: WindowEvent::CursorMoved { position, .. },
                ..
            } => {
                input.handle_mouse_move(position);
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
