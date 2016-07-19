use ::stage::Stage;
use ::fighter::{Fighter, ActionFrame};
use ::player::RenderPlayer;

use glium;
use glium::backend::glutin_backend::GlutinFacade;

use std::f32::consts;

#[derive(Copy, Clone)]
pub struct Vertex {
    pub position: [f32; 2],
}
implement_vertex!(Vertex, position);

pub struct Buffers {
    pub vertex: glium::VertexBuffer<Vertex>,
    pub index: glium::IndexBuffer<u16>,
}

impl Buffers {
    pub fn new(display: &GlutinFacade) -> Buffers {
        Buffers {
            vertex: glium::VertexBuffer::empty_dynamic(display, 1000).unwrap(),
            index: glium::IndexBuffer::empty_dynamic(display, glium::index::PrimitiveType::TrianglesList, 1000).unwrap(),
        }
    }

    pub fn new_stage(display: &GlutinFacade, stage: &Stage) -> Buffers {
        let mut vertices: Vec<Vertex> = vec!();
        let mut indices: Vec<u16> = vec!();
        let mut indice_count = 0;
        for platform in &stage.platforms {
            let x1 = (platform.x - platform.w / 2.0) as f32;
            let y1 = (platform.y - platform.h / 2.0) as f32;
            let x2 = (platform.x + platform.w / 2.0) as f32;
            let y2 = (platform.y + platform.h / 2.0) as f32;

            vertices.push(Vertex { position: [x1, y1] });
            vertices.push(Vertex { position: [x1, y2] });
            vertices.push(Vertex { position: [x2, y1] });
            vertices.push(Vertex { position: [x2, y2] });

            indices.push(indice_count + 0);
            indices.push(indice_count + 1);
            indices.push(indice_count + 2);
            indices.push(indice_count + 1);
            indices.push(indice_count + 2);
            indices.push(indice_count + 3);
            indice_count += 4;
        }

        Buffers {
            vertex: glium::VertexBuffer::new(display, &vertices).unwrap(),
            index: glium::IndexBuffer::new(display, glium::index::PrimitiveType::TrianglesList, &indices).unwrap(),
        }
    }

    fn new_fighter_frame(display: &GlutinFacade, frame: &ActionFrame) -> Buffers {
        let mut vertices: Vec<Vertex> = vec!();
        let mut indices: Vec<u16> = vec!();
        let mut index_count = 0;
        let triangles = 20;

        for hitbox in &frame.hitboxes {
            for point in &hitbox.points {
                // Draw a hitbox, at the point
                // triangles are drawn meeting at the centre, forming a circle
                for i in 0..triangles {
                    let angle: f32 = ((i * 2) as f32) * consts::PI / (triangles as f32);
                    let x = (point.x as f32) + angle.cos() * (hitbox.radius as f32);
                    let y = (point.y as f32) + angle.sin() * (hitbox.radius as f32);
                    vertices.push(Vertex { position: [x, y] });
                    indices.push(index_count);
                    indices.push(index_count + i);
                    indices.push(index_count + (i + 1) % triangles);
                }
            }
            index_count += 20;
        }

        Buffers {
            vertex: glium::VertexBuffer::new(display, &vertices).unwrap(),
            index: glium::IndexBuffer::new(display, glium::index::PrimitiveType::TrianglesList, &indices).unwrap(),
        }
    }

    pub fn new_player(display: &GlutinFacade, player: &RenderPlayer) -> Buffers {
        let ecb_w = player.ecb_w as f32;
        let ecb_y = player.ecb_y as f32;
        let ecb_top = player.ecb_top as f32;
        let ecb_bottom = player.ecb_bottom as f32;

        // ecb
        let vertex0 = Vertex { position: [ 0.0, ecb_y + ecb_bottom] };
        let vertex1 = Vertex { position: [-ecb_w/2.0, ecb_y] };
        let vertex2 = Vertex { position: [ ecb_w/2.0, ecb_y] };
        let vertex3 = Vertex { position: [ 0.0, ecb_y + ecb_top] };

        // horizontal bps
        let vertex4 = Vertex { position: [-4.0,-0.15] };
        let vertex5 = Vertex { position: [-4.0, 0.15] };
        let vertex6 = Vertex { position: [ 4.0,-0.15] };
        let vertex7 = Vertex { position: [ 4.0, 0.15] };

        // vertical bps
        let vertex8  = Vertex { position: [-0.15,-4.0] };
        let vertex9  = Vertex { position: [ 0.15,-4.0] };
        let vertex10 = Vertex { position: [-0.15, 4.0] };
        let vertex11 = Vertex { position: [ 0.15, 4.0] };

        let shape = vec![vertex0, vertex1, vertex2, vertex3, vertex4, vertex5, vertex6, vertex7, vertex8, vertex9, vertex10, vertex11];
        let indices: [u16; 18] = [
            1,  2,  0,
            1,  2,  3,
            4,  5,  6,
            7,  6,  5,
            8,  9,  10,
            11, 10, 13,
        ];

        let vertices = glium::VertexBuffer::new(display, &shape).unwrap();
        let indices = glium::IndexBuffer::new(display, glium::index::PrimitiveType::TrianglesList, &indices).unwrap();

        Buffers {
            vertex: vertices,
            index: indices,
        }
    }
}

pub struct PackageBuffers {
    pub stages: Vec<Buffers>,
    pub fighters: Vec<Vec<Vec<Buffers>>>, // fighters <- actions <- frames
}

impl PackageBuffers {
    pub fn new(display: &GlutinFacade, fighters: &Vec<Fighter>, stages: &Vec<Stage>) -> PackageBuffers {
        let mut fighter_buffers: Vec<Vec<Vec<Buffers>>> = vec!();
        for fighter in fighters {
            let mut action_buffers: Vec<Vec<Buffers>> = vec!();
            for action in &fighter.action_defs {
                let mut frame_buffers: Vec<Buffers> = vec!();
                for frame in &action.frames {
                    frame_buffers.push(Buffers::new_fighter_frame(display, frame));
                }
                action_buffers.push(frame_buffers);
            }
            fighter_buffers.push(action_buffers);
        }

        let mut stage_buffers:   Vec<Buffers> = vec!();
        for stage in stages {
            stage_buffers.push(Buffers::new_stage(display, stage));
        }

        PackageBuffers {
            stages: stage_buffers,
            fighters: fighter_buffers,
        }
    }
}