use rand::Rng;

#[derive(Copy, Clone, Debug)]
pub struct LLA {
    pub lat: f64,
    pub lon: f64,
    pub alt: f64,
}

impl LLA {
    pub fn new_rand(rng: &mut impl Rng) -> Self {
        let lat = rng.gen_range(-90.0..90.0);
        let lon = rng.gen_range(-90.0..90.0);
        let alt = rng.gen_range(100.0..200.0);
        Self { lat, lon, alt }
    }
}

#[derive(Clone, Debug)]
pub enum RawGrpcRequest {
    Basic,
    FlyTo(LLA),
    Waypoint(Vec<LLA>),
}

#[derive(Clone, Debug)]
pub enum GrpcRequest {
    Basic,
    FlyTo(LLA),
    Waypoint(Vec<LLA>),
}

#[derive(Copy, Clone, Debug)]
pub struct TerrainQuery {
    pub lat: f64,
    pub lon: f64,
    pub seq: usize,
}

#[derive(Copy, Clone, Debug)]
pub struct TerrainResponse {
    pub seq: usize,
    pub terrain: LLA,
}
