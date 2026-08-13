//! Render the ancient map both ways, to compare the dressing.
use worldengine::draw::ancient::{draw_ancientmap, AncientMapOptions, AncientStyle};
use worldengine::draw::image::RgbaImage;
use worldengine::numpy::NumpyRng;
use worldengine::plates::{world_gen, WorldGenParams};

fn main() {
    let dir = std::env::args().nth(1).expect("usage: ancient_preview <out-dir>");
    std::fs::create_dir_all(&dir).unwrap();
    let (w, h) = (1024usize, 512);
    let params = WorldGenParams { plate_expansion: 4, ..WorldGenParams::default() };
    let mut rng = NumpyRng::new(28070);
    let world = world_gen("preview", w, h, 28070, &params, &mut rng);

    for (name, opts) in [
        ("plain", AncientMapOptions::default()),
        ("engraved", AncientMapOptions {
            style: AncientStyle::Engraved,
            draw_outer_land_border: true,
            ..AncientMapOptions::default()
        }),
    ] {
        let mut img = RgbaImage::new(w, h);
        draw_ancientmap(&world, &mut img, opts);
        img.write_png(format!("{dir}/{name}.png")).unwrap();
        println!("wrote {dir}/{name}.png");
    }
}
