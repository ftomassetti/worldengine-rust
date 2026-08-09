//! Port of `worldengine/simulations/erosion.py`.
//!
//! Traces rainfall downhill into rivers, carves valleys along their courses and
//! records the resulting river and lake maps.

use crate::astar;
use crate::matrix::Matrix;
use crate::world::World;

// Directions.
const NORTH: [i64; 2] = [0, -1];
const EAST: [i64; 2] = [1, 0];
const SOUTH: [i64; 2] = [0, 1];
const WEST: [i64; 2] = [-1, 0];
const CENTER: [i64; 2] = [0, 0];

const DIR_NEIGHBORS: [[i64; 2]; 4] = [NORTH, EAST, SOUTH, WEST];
const DIR_NEIGHBORS_CENTER: [[i64; 2]; 5] = [CENTER, NORTH, EAST, SOUTH, WEST];

const RIVER_TH: f64 = 0.02;

/// Python's `%` on a negative left operand returns a non-negative result, which
/// `i64::rem_euclid` reproduces. Rust's own `%` would return a negative value
/// and index out of bounds — this function is called with -1 at map edges.
fn overflow(value: i64, max_value: i64) -> i64 {
    value.rem_euclid(max_value)
}

fn in_circle(radius: i64, center_x: i64, center_y: i64, x: i64, y: i64) -> bool {
    let square_dist = (center_x - x).pow(2) + (center_y - y).pow(2);
    square_dist <= radius.pow(2)
}

pub fn is_applicable(_world: &World) -> bool {
    true
}

pub fn execute(world: &mut World, _seed: u32) {
    let (height, width) = (world.height, world.width);
    let mut water_flow = Matrix::<f64>::new(width, height);
    let mut water_path = Matrix::<usize>::new(width, height);
    let mut river_list: Vec<Vec<[i64; 2]>> = Vec::new();
    let mut lake_list: Vec<[i64; 2]> = Vec::new();
    let mut river_map = Matrix::<f64>::new(width, height);
    let mut lake_map = Matrix::<f64>::new(width, height);

    // Step one: water flow per cell based on rainfall.
    find_water_flow(world, &mut water_path);

    // Step two: find river sources (seeds).
    let river_sources = river_sources(world, &mut water_flow, &water_path);

    // Step three: for each source, find a path to the sea.
    for source in river_sources {
        let river = river_flow(source, world, &river_list, &mut lake_list);
        if !river.is_empty() {
            clean_up_flow(&river, world);
            let last = *river.last().unwrap();
            let (rx, ry) = (last[0] as usize, last[1] as usize);
            river_list.push(river);
            if !world.is_ocean((rx, ry)) {
                lake_list.push(last); // The river flowed into a lake.
            }
        }
    }

    // Step four: simulate erosion and update the river map.
    for river in &river_list {
        river_erosion(river, world);
        rivermap_update(river, &water_flow, &mut river_map, &world.precipitation_layer().data);
    }

    // Step five: rivers with no path to the sea form lakes.
    for lake in &lake_list {
        lake_map[(lake[1] as usize, lake[0] as usize)] = 0.1;
    }

    world.river_map = Some(river_map);
    world.lake_map = Some(lake_map);
}

/// Find the flow direction for each cell in the height map.
fn find_water_flow(world: &World, water_path: &mut Matrix<usize>) {
    for x in 0..world.width.saturating_sub(1) {
        for y in 0..world.height.saturating_sub(1) {
            if let Some(path) = find_quick_path([x as i64, y as i64], world) {
                let flow_dir = [path[0] - x as i64, path[1] - y as i64];
                for (key, direction) in DIR_NEIGHBORS_CENTER.iter().enumerate() {
                    if *direction == flow_dir {
                        water_path[(y, x)] = key;
                    }
                }
            }
        }
    }
}

/// Water flows based on cost, seeking the highest elevation difference; the
/// lowest neighbour is the path of least resistance.
fn find_quick_path(river: [i64; 2], world: &World) -> Option<[i64; 2]> {
    let (x, y) = (river[0], river[1]);
    let elevation_data = world.elevation_data();
    let mut new_path: Option<[i64; 2]> = None;
    let mut lowest_elevation = elevation_data[(y as usize, x as usize)];

    for [dx, dy] in DIR_NEIGHBORS {
        // `wrap` is always true in the Python constructor.
        let tx = overflow(x + dx, world.width as i64);
        let ty = overflow(y + dy, world.height as i64);

        let elevation = elevation_data[(ty as usize, tx as usize)];
        if elevation < lowest_elevation {
            lowest_elevation = elevation;
            new_path = Some([tx, ty]);
        }
    }

    new_path
}

/// Find the places on the map where rivers can begin.
///
/// Using the flow directions, follow the path from each cell, adding the
/// previous cell's flow to the current cell's. Cells above the flow threshold
/// that are still above sea level become sources.
fn river_sources(
    world: &World,
    water_flow: &mut Matrix<f64>,
    water_path: &Matrix<usize>,
) -> Vec<[i64; 2]> {
    let mut river_source_list: Vec<[i64; 2]> = Vec::new();
    let precipitation = &world.precipitation_layer().data;

    for y in 0..world.height.saturating_sub(1) {
        for x in 0..world.width.saturating_sub(1) {
            let rain_fall = precipitation[(y, x)];
            water_flow[(y, x)] = rain_fall;

            if water_path[(y, x)] == 0 {
                continue; // Ignore cells without a flow direction.
            }
            let (mut cx, mut cy) = (x, y);
            let mut neighbour_seed_found = false;
            // Follow the flow path to wherever it leads.
            while !neighbour_seed_found {
                if world.is_mountain((cx, cy)) && water_flow[(cy, cx)] >= RIVER_TH {
                    // Try not to create seeds around other seeds.
                    for seed in &river_source_list {
                        if in_circle(9, cx as i64, cy as i64, seed[0], seed[1]) {
                            neighbour_seed_found = true;
                        }
                    }
                    if neighbour_seed_found {
                        break;
                    }
                    river_source_list.push([cx as i64, cy as i64]);
                    break;
                }

                if water_path[(cy, cx)] == 0 {
                    break; // A dead end.
                }

                // Follow the path, adding the water flow from the previous cell.
                let [dx, dy] = DIR_NEIGHBORS_CENTER[water_path[(cy, cx)]];
                let nx = (cx as i64 + dx) as usize;
                let ny = (cy as i64 + dy) as usize;
                water_flow[(ny, nx)] += rain_fall;
                cx = nx;
                cy = ny;
            }
        }
    }

    river_source_list
}

/// Simulate fluid dynamics: start at the source and flow to the lowest
/// available point.
fn river_flow(
    source: [i64; 2],
    world: &World,
    river_list: &[Vec<[i64; 2]>],
    lake_list: &mut Vec<[i64; 2]>,
) -> Vec<[i64; 2]> {
    let mut current_location = source;
    let mut path = vec![source];

    loop {
        let (x, y) = (current_location[0], current_location[1]);

        // Is there a river nearby? Flow into it.
        for [dx, dy] in DIR_NEIGHBORS {
            let ax = overflow(x + dx, world.width as i64);
            let ay = overflow(y + dy, world.height as i64);

            for river in river_list {
                if river.contains(&[ax, ay]) {
                    let mut merge = false;
                    for &[rx, ry] in river {
                        if [ax, ay] == [rx, ry] {
                            merge = true;
                            path.push([rx, ry]);
                        } else if merge {
                            path.push([rx, ry]);
                        }
                    }
                    return path; // Skip the rest.
                }
            }
        }

        // Found a sea?
        if world.is_ocean((x as usize, y as usize)) {
            break;
        }

        // Find the immediate lowest elevation and flow there.
        if let Some(quick_section) = find_quick_path(current_location, world) {
            path.push(quick_section);
            current_location = quick_section;
            continue;
        }

        let (is_wrapped, lower_elevation) = find_lower_elevation(current_location, world);
        match (lower_elevation, is_wrapped) {
            (Some(lower), false) => {
                let lower_path = astar::find_path(
                    world.elevation_data(),
                    (current_location[0], current_location[1]),
                    (lower[0], lower[1]),
                );
                if !lower_path.is_empty() {
                    path.extend(lower_path);
                    current_location = *path.last().unwrap();
                } else {
                    break;
                }
            }
            (Some(lower), true) => {
                let max_radius = 40;
                let (cx, cy) = (current_location[0], current_location[1]);
                let (mut lx, mut ly) = (lower[0], lower[1]);
                let (nx, ny);

                if !in_circle(max_radius, cx, cy, lx, cy) {
                    // Wrapping on the x axis.
                    if cx - lx < 0 {
                        lx = 0; // Move to the left edge...
                        nx = world.width as i64 - 1; // ...and step wrapped around.
                    } else {
                        lx = world.width as i64 - 1;
                        nx = 0;
                    }
                    ly = (cy + ly) / 2; // Move halfway.
                    ny = ly;
                } else if !in_circle(max_radius, cx, cy, cx, ly) {
                    // Wrapping on the y axis.
                    if cy - ly < 0 {
                        ly = 0;
                        ny = world.height as i64 - 1;
                    } else {
                        ly = world.height as i64 - 1;
                        ny = 0;
                    }
                    lx = (cx + lx) / 2;
                    nx = lx;
                } else {
                    panic!("BUG: fix me... we are not in circle: {current_location:?} {lower:?}");
                }

                // Find our way to the edge.
                let edge_path = astar::find_path(world.elevation_data(), (cx, cy), (lx, ly));
                if edge_path.is_empty() {
                    // No other path: make it a lake.
                    lake_list.push(current_location);
                    break;
                }
                path.extend(edge_path);
                path.push([nx, ny]); // Add the overflow to the other side.
                current_location = *path.last().unwrap();

                // Find our way to the lowest position originally found.
                let lower_path = astar::find_path(
                    world.elevation_data(),
                    (current_location[0], current_location[1]),
                    (lower[0], lower[1]),
                );
                path.extend(lower_path);
                current_location = *path.last().unwrap();
            }
            _ => {
                // Can't find any other path: make it a lake.
                lake_list.push(current_location);
                break;
            }
        }
    }

    path
}

/// Validate that each point in a river is equal to or lower than the last.
fn clean_up_flow(river: &[[i64; 2]], world: &mut World) {
    let mut celevation = 1.0f64;
    let data = &mut world.elevation.as_mut().unwrap().data;
    for r in river {
        let (rx, ry) = (r[0] as usize, r[1] as usize);
        let relevation = data[(ry, rx)];
        if relevation <= celevation {
            celevation = relevation;
        } else {
            data[(ry, rx)] = celevation;
        }
    }
}

/// Look for a lower elevation within an increasing circle's radius.
fn find_lower_elevation(source: [i64; 2], world: &World) -> (bool, Option<[i64; 2]>) {
    let (x, y) = (source[0], source[1]);
    let mut current_radius = 1i64;
    let max_radius = 40i64;
    let elevation_data = world.elevation_data();
    let mut lowest_elevation = elevation_data[(y as usize, x as usize)];
    let mut destination: Option<[i64; 2]> = None;
    let mut not_found = true;
    let mut wrapped: Vec<[i64; 2]> = Vec::new();

    while not_found && current_radius <= max_radius {
        for cx in -current_radius..=current_radius {
            for cy in -current_radius..=current_radius {
                let (rx0, ry0) = (x + cx, y + cy);

                // Are we within the circle?
                if !in_circle(current_radius, x, y, rx0, ry0) {
                    continue;
                }

                let rx = overflow(rx0, world.width as i64);
                let ry = overflow(ry0, world.height as i64);

                let elevation = elevation_data[(ry as usize, rx as usize)];
                if elevation < lowest_elevation {
                    lowest_elevation = elevation;
                    destination = Some([rx, ry]);
                    not_found = false;
                    if !world.contains((rx0, ry0)) {
                        wrapped.push([rx, ry]);
                    }
                }
            }
        }
        current_radius += 1;
    }

    let is_wrapped = destination.is_some_and(|d| wrapped.contains(&d));
    (is_wrapped, destination)
}

/// Simulate erosion in the height map based on a river path: the riverbed is
/// carved out and the sides are eroded to slope into it.
fn river_erosion(river: &[[i64; 2]], world: &mut World) {
    for r in river {
        let (rx, ry) = (r[0], r[1]);
        let radius = 2i64;
        // Note the Python's ranges are exclusive at the top, so this sweeps
        // rx-2..=rx+1 rather than a symmetric window.
        for x0 in (rx - radius)..(rx + radius) {
            for y0 in (ry - radius)..(ry + radius) {
                let x = overflow(x0, world.width as i64);
                let y = overflow(y0, world.height as i64);
                let mut curve = 1.0f64;

                // The Python compares against [0, 0] rather than the river cell
                // — an apparent typo, preserved.
                if [x, y] == [0, 0] {
                    continue;
                }
                if river.contains(&[x, y]) {
                    continue; // Ignore the river itself.
                }

                let data = &world.elevation_layer().data;
                let cell = data[(y as usize, x as usize)];
                let river_elev = data[(ry as usize, rx as usize)];
                if cell <= river_elev {
                    continue; // Ignore areas lower than the river itself.
                }
                if !in_circle(radius, rx, ry, x, y) {
                    continue;
                }

                let adx = (rx - x).abs();
                let ady = (ry - y).abs();
                if adx == 1 || ady == 1 {
                    curve = 0.2;
                } else if adx == 2 || ady == 2 {
                    curve = 0.05;
                }

                let diff = river_elev - cell;
                let new_elevation = cell + (diff * curve);
                if new_elevation <= river_elev {
                    // The Python prints "newElevation is <= than river, fix
                    // me..." and then evaluates `data[r, x]` with `r` a
                    // two-element list, which numpy turns into fancy indexing
                    // and the subsequent assignment raises. In other words the
                    // original crashes here; there is no correct behaviour to
                    // copy.
                    panic!(
                        "river erosion reached the unimplemented branch of the original \
                         (new elevation {new_elevation} <= river elevation {river_elev})"
                    );
                }
                world.elevation.as_mut().unwrap().data[(y as usize, x as usize)] = new_elevation;
            }
        }
    }
}

/// Update the river map with the rainfall that becomes the water flow.
fn rivermap_update(
    river: &[[i64; 2]],
    water_flow: &Matrix<f64>,
    rivermap: &mut Matrix<f64>,
    precipitations: &Matrix<f64>,
) {
    let mut is_seed = true;
    let (mut px, mut py) = (0usize, 0usize);
    for &[x, y] in river {
        let (x, y) = (x as usize, y as usize);
        if is_seed {
            rivermap[(y, x)] = water_flow[(y, x)];
            is_seed = false;
        } else {
            rivermap[(y, x)] = precipitations[(y, x)] + rivermap[(py, px)];
        }
        px = x;
        py = y;
    }
}
