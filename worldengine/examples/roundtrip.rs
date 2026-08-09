//! Generate a small world and write it as a `.world` file, so the Python
//! worldengine can be asked to read it back.
use worldengine::numpy::NumpyRng;
use worldengine::plates::{world_gen, WorldGenParams};
use worldengine::serialization::protobuf_to_file;

fn main() {
    let mut rng = NumpyRng::new(28070);
    let world = world_gen("seed_28070", 96, 48, 28070, &WorldGenParams::default(), &mut rng);
    let path = std::env::args().nth(1).unwrap_or_else(|| "/tmp/rust.world".into());
    protobuf_to_file(&world, &path).unwrap();
    println!("wrote {path}");
}
