use ::vulkan_buffers::{Vertex, Buffers, PackageBuffers};
use ::game::{GameState, RenderEntity, RenderGame};
use ::menu::RenderMenu;
use ::graphics::{GraphicsMessage, Render};
use ::player::RenderFighter;

use vulkano_win;
use vulkano_win::VkSurfaceBuild;
use vulkano;
use vulkano::buffer::{BufferUsage, CpuAccessibleBuffer};
use vulkano::command_buffer::{PrimaryCommandBufferBuilder, Submission, DynamicState};
use vulkano::command_buffer;
use vulkano::descriptor::descriptor_set::DescriptorPool;
use vulkano::device::{Device, Queue};
use vulkano::framebuffer::{Framebuffer, Subpass};
use vulkano::image::SwapchainImage;
use vulkano::instance::{Instance, PhysicalDevice};
use vulkano::pipeline::blend::Blend;
use vulkano::pipeline::depth_stencil::DepthStencil;
use vulkano::pipeline::input_assembly::InputAssembly;
use vulkano::pipeline::multisample::Multisample;
use vulkano::pipeline::vertex::SingleBufferDefinition;
use vulkano::pipeline::viewport::{ViewportsState, Viewport, Scissor};
use vulkano::pipeline::{GraphicsPipeline, GraphicsPipelineParams};
use vulkano::swapchain::{Swapchain, SurfaceTransform};
use winit::{Event, WindowBuilder};

use std::sync::Arc;
use std::sync::mpsc::{Sender, Receiver, channel};
use std::thread;
use std::time::Duration;

mod generic_vs { include!{concat!(env!("OUT_DIR"), "/shaders/src/shaders/generic-vertex.glsl")} }
mod generic_fs { include!{concat!(env!("OUT_DIR"), "/shaders/src/shaders/generic-fragment.glsl")} }

mod render_pass {
    use vulkano::format::Format;
    single_pass_renderpass!{
        attachments: {
            color: {
                load:   Clear,
                store:  Store,
                format: Format,
            }
        },
        pass: {
            color: [color],
            depth_stencil: {}
        }
    }
}

mod generic_pipeline_layout {
    pipeline_layout! {
        set0: {
            uniforms: UniformBuffer<::vulkan::generic_vs::ty::Data>
        }
    }
}

pub struct Uniform {
    uniform:  Arc<CpuAccessibleBuffer<generic_vs::ty::Data>>,
    set:      Arc<generic_pipeline_layout::set0::Set>,
}

#[allow(dead_code)]
pub struct VulkanGraphics {
    package_buffers:  PackageBuffers,
    window:           vulkano_win::Window,
    device:           Arc<Device>,
    swapchain:        Arc<Swapchain>,
    queue:            Arc<Queue>,
    submissions:      Vec<Arc<Submission>>,
    generic_pipeline: Arc<GraphicsPipeline<SingleBufferDefinition<Vertex>, generic_pipeline_layout::CustomPipeline, render_pass::CustomRenderPass>>,
    render_pass:      Arc<render_pass::CustomRenderPass>,
    framebuffers:     Vec<Arc<Framebuffer<render_pass::CustomRenderPass>>>,
    uniforms:         Vec<Uniform>,
    os_input_tx:      Sender<Event>,
    render_rx:        Receiver<GraphicsMessage>,
}

impl VulkanGraphics {
    pub fn init(os_input_tx: Sender<Event>) -> Sender<GraphicsMessage> {
        let (render_tx, render_rx) = channel();

        thread::spawn(move || {
            let mut graphics = VulkanGraphics::new(os_input_tx, render_rx);
            graphics.run();
        });
        render_tx
    }

    fn new(os_input_tx: Sender<Event>, render_rx: Receiver<GraphicsMessage>) -> VulkanGraphics {
        let instance = {
            let extensions = vulkano_win::required_extensions();
            Instance::new(None, &extensions, None).expect("failed to create Vulkan instance")
        };

        let physical = PhysicalDevice::enumerate(&instance).next().expect("no device available");
        let window  = WindowBuilder::new().build_vk_surface(&instance).unwrap();
        window.window().set_title("PF ENGINE");

        let queue = physical.queue_families().find(|q| {
            q.supports_graphics() && window.surface().is_supported(q).unwrap_or(false)
        }).unwrap();

        let (device, mut queues) = {
            let device_ext = vulkano::device::DeviceExtensions {
                khr_swapchain: true,
                .. vulkano::device::DeviceExtensions::none()
            };
            Device::new(&physical, physical.supported_features(), &device_ext, [(queue, 0.5)].iter().cloned()).unwrap()
        };

        let queue = queues.next().unwrap();

        let (swapchain, images) = {
            let caps = window.surface().get_capabilities(&physical).unwrap();
            let dimensions = caps.current_extent.unwrap_or([640, 480]);
            let present = caps.present_modes.iter().next().unwrap();
            let alpha = caps.supported_composite_alpha.iter().next().unwrap();
            let format = caps.supported_formats[0].0;
            Swapchain::new(&device, &window.surface(), caps.min_image_count, format, dimensions, 1,
                &caps.supported_usage_flags, &queue, SurfaceTransform::Identity, alpha, present, true, None
            ).unwrap()
        };

        let render_pass = render_pass::CustomRenderPass::new(
            &device, &render_pass::Formats { color: (images[0].format(), 1) }
        ).unwrap();

        let framebuffers = images.iter().map(|image| {
            let dimensions = [image.dimensions()[0], image.dimensions()[1], 1];
            Framebuffer::new(&render_pass, dimensions, render_pass::AList {
                color: image
            }).unwrap()
        }).collect::<Vec<_>>();

        let (uniforms, generic_pipeline) = VulkanGraphics::generic_pipeline(&device, &queue, &images, &render_pass);

        VulkanGraphics {
            package_buffers:  PackageBuffers::new(),
            window:           window,
            device:           device,
            swapchain:        swapchain,
            queue:            queue,
            submissions:      vec!(),
            generic_pipeline: generic_pipeline,
            render_pass:      render_pass,
            framebuffers:     framebuffers,
            uniforms:         uniforms,
            os_input_tx:      os_input_tx,
            render_rx:        render_rx,
        }
    }

    fn generic_pipeline(
        device: &Arc<Device>,
        queue: &Arc<Queue>,
        images: &Vec<Arc<SwapchainImage>>,
        render_pass: &Arc<render_pass::CustomRenderPass>
    ) -> (
        Vec<Uniform>,
        Arc<GraphicsPipeline<SingleBufferDefinition<Vertex>, generic_pipeline_layout::CustomPipeline, render_pass::CustomRenderPass>>
    ) {
        let pipeline_layout = generic_pipeline_layout::CustomPipeline::new(&device).unwrap();

        let vs = generic_vs::Shader::load(&device).unwrap();
        let fs = generic_fs::Shader::load(&device).unwrap();

        let mut uniforms: Vec<Uniform> = vec!();
        for _ in 0..1000 {
            let uniform = CpuAccessibleBuffer::<generic_vs::ty::Data>::from_data(
                &device,
                &BufferUsage::all(),
                Some(queue.family()),
                generic_vs::ty::Data {
                    position_offset: [0.0, 0.0],
                    zoom:            1.0,
                    aspect_ratio:    1.0,
                    direction:       1.0,
                    edge_color:      [1.0, 1.0, 1.0, 1.0],
                    color:           [1.0, 1.0, 1.0, 1.0],
                    _dummy0:         [0; 12],
                }
            ).unwrap();

            let descriptor_pool = DescriptorPool::new(&device);
            let set = generic_pipeline_layout::set0::Set::new(&descriptor_pool, &pipeline_layout, &generic_pipeline_layout::set0::Descriptors {
                uniforms: &uniform
            });
            uniforms.push(Uniform {
                uniform: uniform,
                set: set
            });
        }

        let pipeline = GraphicsPipeline::new(&device,
            GraphicsPipelineParams {
                vertex_input:    SingleBufferDefinition::new(),
                vertex_shader:   vs.main_entry_point(),
                input_assembly:  InputAssembly::triangle_list(),
                tessellation:    None,
                geometry_shader: None,
                viewport:        ViewportsState::Fixed {
                    data: vec![(
                        Viewport {
                            origin:      [0.0, 0.0],
                            depth_range: 0.0..1.0,
                            dimensions:  [
                                images[0].dimensions()[0] as f32,
                                images[0].dimensions()[1] as f32
                            ],
                        },
                        Scissor::irrelevant()
                    )],
                },
                raster:          Default::default(),
                multisample:     Multisample::disabled(),
                fragment_shader: fs.main_entry_point(),
                depth_stencil:   DepthStencil::disabled(),
                blend:           Blend::alpha_blending(),
                layout:          &pipeline_layout,
                render_pass:     Subpass::from(&render_pass, 0).unwrap(),
            }
        ).unwrap();

        (uniforms, pipeline)
    }

    fn run(&mut self) {
        loop {
            self.submissions.retain(|s| s.destroying_would_block());
            {
                // get the most recent render
                let mut render = {
                    let message = self.render_rx.recv().unwrap();
                    self.read_message(message)
                };
                while let Ok(message) = self.render_rx.try_recv() {
                    render = self.read_message(message);
                }

                match render {
                    Render::Game(game) => { self.game_render(game); },
                    Render::Menu(menu) => { self.menu_render(menu); },
                }
            }
            self.handle_events();
        }
    }

    fn read_message(&mut self, message: GraphicsMessage) -> Render {
        self.package_buffers.update(&self.device, &self.queue, message.package_updates);
        message.render
    }

    fn game_render(&mut self, render: RenderGame) {
        let image_num = self.swapchain.acquire_next_image(Duration::new(1, 0)).unwrap();
        let mut command_buffer = PrimaryCommandBufferBuilder::new(&self.device, self.queue.family())
        .draw_inline(&self.render_pass, &self.framebuffers[image_num], render_pass::ClearValues {
            color: [0.0, 0.0, 0.0, 1.0]
        });

        let mut uniforms = self.uniforms.iter();
        let zoom = render.camera.zoom.recip();
        let pan  = render.camera.pan;
        let (width, height) = self.window.window().get_inner_size_points().unwrap();
        let aspect_ratio = width as f32 / height as f32;

        match render.state {
            GameState::Local  => { },
            GameState::Paused => {
                // TODO: blue vaporwavey background lines to indicate pause :D
                // also double as measuring/scale lines
                // configurable size via treeflection
                // but this might be desirable to have during normal gameplay to, hmmmm....
            },
            _ => { },
        }

        let stage = 0;
        let uniform = uniforms.next().unwrap();
        {
            let mut buffer_content = uniform.uniform.write(Duration::new(1, 0)).unwrap();
            buffer_content.zoom            = zoom;
            buffer_content.aspect_ratio    = aspect_ratio;
            buffer_content.position_offset = [pan.0 as f32, pan.1 as f32];
            buffer_content.direction       = 1.0;
            buffer_content.edge_color      = [1.0, 1.0, 1.0, 1.0];
            buffer_content.color           = [1.0, 1.0, 1.0, 1.0];
        }
        let vertex_buffer = &self.package_buffers.stages[stage].vertex;
        let index_buffer  = &self.package_buffers.stages[stage].index;
        command_buffer = command_buffer.draw_indexed(&self.generic_pipeline, vertex_buffer, index_buffer, &DynamicState::none(), &uniform.set, &());

        for entity in render.entities {
            match entity {
                RenderEntity::Player(player) => {
                    let dir = if player.face_right { 1.0 } else { -1.0 } as f32;
                    let draw_pos = [player.bps.0 + pan.0 as f32, player.bps.1 + pan.1 as f32];
                    // draw player ecb
                    if player.debug.ecb {
                        let buffers = Buffers::new_player(&self.device, &self.queue, &player);
                        let uniform = uniforms.next().unwrap();
                        {
                            let mut buffer_content = uniform.uniform.write(Duration::new(1, 0)).unwrap();
                            buffer_content.zoom            = zoom;
                            buffer_content.aspect_ratio    = aspect_ratio;
                            buffer_content.position_offset = draw_pos;
                            buffer_content.direction       = dir;
                            buffer_content.edge_color      = [0.0, 1.0, 0.0, 1.0];
                            if player.fighter_selected {
                                buffer_content.color = [0.0, 1.0, 0.0, 1.0];
                            }
                            else {
                                buffer_content.color = [1.0, 1.0, 1.0, 1.0];
                            }
                        }
                        command_buffer = command_buffer.draw_indexed(&self.generic_pipeline, &buffers.vertex, &buffers.index, &DynamicState::none(), &uniform.set, &());
                    }

                    // setup fighter uniform
                    match player.debug.fighter {
                        RenderFighter::Normal | RenderFighter::Debug => {
                            let uniform = uniforms.next().unwrap();
                            {
                                let mut buffer_content = uniform.uniform.write(Duration::new(1, 0)).unwrap();
                                buffer_content.zoom            = zoom;
                                buffer_content.aspect_ratio    = aspect_ratio;
                                buffer_content.position_offset = draw_pos;
                                buffer_content.direction       = dir;
                                if let RenderFighter::Debug = player.debug.fighter {
                                    buffer_content.color = [0.0, 0.0, 0.0, 0.0];
                                }
                                else {
                                    buffer_content.color = [1.0, 1.0, 1.0, 1.0];
                                }
                                if player.fighter_selected {
                                    buffer_content.edge_color = [0.0, 1.0, 0.0, 1.0];
                                }
                                else {
                                    buffer_content.edge_color = player.fighter_color;
                                }
                            }

                            // draw fighter
                            let fighter_frames = &self.package_buffers.fighters[player.fighter][player.action];
                            if player.frame < fighter_frames.len() {
                                if let &Some(ref buffers) = &fighter_frames[player.frame] {
                                    command_buffer = command_buffer.draw_indexed(&self.generic_pipeline, &buffers.vertex, &buffers.index, &DynamicState::none(), &uniform.set, &());
                                }
                            }
                            else {
                                 //TODO: Give some indication that we are rendering a deleted or otherwise nonexistent frame
                            }
                        }
                        RenderFighter::None => { }
                    }

                    // draw selected hitboxes
                    if player.selected_colboxes.len() > 0 {
                        // I could store which element each vertex is part of and handle this in the shader but then I wouldn't be able to highlight overlapping elements.
                        // The extra vertex generation + draw should be fast enough (this only occurs on the pause screen)
                        let uniform = uniforms.next().unwrap();
                        {
                            let mut buffer_content = uniform.uniform.write(Duration::new(1, 0)).unwrap();
                            buffer_content.zoom            = zoom;
                            buffer_content.aspect_ratio    = aspect_ratio;
                            buffer_content.position_offset = [player.bps.0 + pan.0 as f32, player.bps.1 + pan.1 as f32];
                            buffer_content.direction       = if player.face_right { 1.0 } else { -1.0 } as f32;
                            buffer_content.edge_color      = [0.0, 1.0, 0.0, 1.0];
                            buffer_content.color           = [0.0, 1.0, 0.0, 1.0];
                        }
                        let buffers = self.package_buffers.fighter_frame_colboxes(&self.device, &self.queue, player.fighter, player.action, player.frame, &player.selected_colboxes);
                        command_buffer = command_buffer.draw_indexed(&self.generic_pipeline, &buffers.vertex, &buffers.index, &DynamicState::none(), &uniform.set, &());
                    }

                    // TODO: Edit::Player  - render selected player's BPS as green
                    // TODO: Edit::Fighter - Click and drag on ECB points
                    // TODO: Edit::Stage   - render selected platforms as green
                },
                RenderEntity::Selector(rect) => {
                    let uniform = uniforms.next().unwrap();
                    {
                        let mut buffer_content = uniform.uniform.write(Duration::new(1, 0)).unwrap();
                        buffer_content.zoom            = zoom;
                        buffer_content.aspect_ratio    = aspect_ratio;
                        buffer_content.position_offset = [pan.0 as f32, pan.1 as f32];
                        buffer_content.direction       = 1.0;
                        buffer_content.edge_color      = [0.0, 1.0, 0.0, 1.0];
                        buffer_content.color           = [0.0, 1.0, 0.0, 1.0];
                    }
                    let buffers = Buffers::rect_buffers(&self.device, &self.queue, rect);
                    command_buffer = command_buffer.draw_indexed(&self.generic_pipeline, &buffers.vertex, &buffers.index, &DynamicState::none(), &uniform.set, &());
                },
                RenderEntity::Area(rect) => {
                    let uniform = uniforms.next().unwrap();
                    {
                        let mut buffer_content = uniform.uniform.write(Duration::new(1, 0)).unwrap();
                        buffer_content.zoom            = zoom;
                        buffer_content.aspect_ratio    = aspect_ratio;
                        buffer_content.position_offset = [pan.0 as f32, pan.1 as f32];
                        buffer_content.direction       = 1.0;
                        buffer_content.edge_color      = [0.0, 1.0, 0.0, 1.0];
                        buffer_content.color           = [0.0, 1.0, 0.0, 1.0]; // HMMM maybe i can use only the edge to get the outline from a normal rect?
                    }
                    let buffers = Buffers::rect_buffers(&self.device, &self.queue, rect);
                    command_buffer = command_buffer.draw_indexed(&self.generic_pipeline, &buffers.vertex, &buffers.index, &DynamicState::none(), &uniform.set, &());
                },
            }
        }

        let final_command_buffer = command_buffer.draw_end().build();
        self.submissions.push(command_buffer::submit(&final_command_buffer, &self.queue).unwrap());
        self.swapchain.present(&self.queue, image_num).unwrap();
    }

    #[allow(unused_variables)]
    fn menu_render(&mut self, render: RenderMenu) {
    }

    fn handle_events(&mut self) {
        // force send the current resolution
        let window = self.window.window();
        let res = window.get_inner_size_points().unwrap();
        self.os_input_tx.send(Event::Resized(res.0, res.1)).unwrap();

        for ev in window.poll_events() {
            self.os_input_tx.send(ev).unwrap();
        }
    }
}