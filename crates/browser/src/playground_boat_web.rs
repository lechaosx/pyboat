use crate::{ClothMotion, PlaygroundBoat, controller_lab::LabControllerKind};
use rapier2d_f64::glamx::Vec2;
use simulation::WorldGeometry;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct PlaygroundBoatDriver {
    boat: PlaygroundBoat,
    cloth: ClothMotion,
    vertices: Vec<u8>,
    batches: Vec<u32>,
}

#[wasm_bindgen]
impl PlaygroundBoatDriver {
    #[wasm_bindgen(constructor)]
    pub fn new(
        width: f64,
        height: f64,
        seed: u64,
        tick: u64,
        x: f64,
        y: f64,
    ) -> Result<Self, JsValue> {
        let geometry = WorldGeometry::new(width, height)
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        let boat = PlaygroundBoat::spawn(
            geometry,
            seed,
            tick,
            rapier2d_f64::prelude::Vector::new(x, y),
        )
        .ok_or_else(|| {
            JsValue::from_str("No safe water nearby; pan toward the sea and try again")
        })?;
        let cloth = ClothMotion::new(&boat.scene());
        Ok(Self {
            boat,
            cloth,
            vertices: Vec::new(),
            batches: Vec::new(),
        })
    }
    pub fn select(&mut self, kind: u32) {
        self.boat
            .select(LabControllerKind::ALL[kind.min(4) as usize]);
    }
    pub fn rebase(&mut self, width: f64, height: f64, seed: u64, tick: u64) -> Result<(), JsValue> {
        let geometry = WorldGeometry::new(width, height)
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        self.boat.rebase(geometry, seed, tick);
        Ok(())
    }
    pub fn step(&mut self, forward: f32, turn: f32, world_x: f32, world_y: f32) {
        self.boat
            .set_player_input([forward, turn, world_x, world_y]);
        self.boat.step();
        self.cloth
            .advance(&self.boat.scene(), simulation::DT_SECONDS);
    }
    pub fn state(&self) -> Vec<f64> {
        let m = self.boat.motion();
        vec![
            m.position_m.x,
            m.position_m.y,
            m.angle_rad,
            m.velocity_m_s.length(),
            m.angular_velocity_rad_s,
            self.boat.intent().forward,
            self.boat.intent().turn,
            m.sail_angle_rad,
            m.rudder_angle_rad,
            self.boat.tick() as f64,
            self.boat.environment().water_depth_m,
        ]
    }
    pub fn prepare_mesh(
        &mut self,
        camera_x: f32,
        camera_y: f32,
        output_height: u32,
        view_height: f32,
        time_seconds: f64,
    ) {
        let (vertices, batches) = crate::boat_sprites::sprite_vertices(
            &[self
                .boat
                .scene_near_camera(rapier2d_f64::prelude::Vector::new(
                    f64::from(camera_x),
                    f64::from(camera_y),
                ))],
            &[self.cloth.pose()],
            self.boat.geometry,
            self.boat.seed,
            Vec2::new(camera_x, camera_y),
            output_height,
            view_height,
            time_seconds,
        );
        self.vertices = bytemuck::cast_slice(&vertices).to_vec();
        self.batches = batches
            .iter()
            .flat_map(|batch| {
                [
                    batch.image as u32,
                    u32::from(batch.sail),
                    batch.vertices.start,
                    batch.vertices.end,
                ]
            })
            .collect();
    }
    pub fn vertices(&self) -> Vec<u8> {
        self.vertices.clone()
    }
    pub fn batches(&self) -> Vec<u32> {
        self.batches.clone()
    }
}

#[wasm_bindgen]
pub fn playground_boat_shader_source() -> String {
    include_str!(concat!(env!("OUT_DIR"), "/boat-sprite.wgsl")).to_owned()
}

#[wasm_bindgen]
pub fn playground_boat_image_count() -> u32 {
    crate::BOAT_IMAGES.len() as u32
}

#[wasm_bindgen]
pub struct PlaygroundBoatImage {
    mips: Vec<crate::SpriteMip>,
}

#[wasm_bindgen]
impl PlaygroundBoatImage {
    #[wasm_bindgen(constructor)]
    pub fn new(index: u32) -> Result<Self, JsValue> {
        let mut decoder =
            png::Decoder::new(std::io::Cursor::new(crate::BOAT_IMAGES[index as usize]));
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder
            .read_info()
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        let mut pixels = vec![
            0;
            reader
                .output_buffer_size()
                .ok_or_else(|| JsValue::from_str("Sprite too large"))?
        ];
        let info = reader
            .next_frame(&mut pixels)
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        Ok(Self {
            mips: crate::sprite_mip_chain(info.width, info.height, &pixels[..info.buffer_size()]),
        })
    }
    pub fn levels(&self) -> u32 {
        self.mips.len() as u32
    }
    pub fn width(&self, level: u32) -> u32 {
        self.mips[level as usize].width
    }
    pub fn height(&self, level: u32) -> u32 {
        self.mips[level as usize].height
    }
    pub fn pixels(&self, level: u32) -> Vec<u8> {
        self.mips[level as usize].rgba.clone()
    }
}
