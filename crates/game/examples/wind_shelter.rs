use std::{env, fmt::Write, fs};

use pyboat_game::{
    procedural_match_environment, procedural_terrain_elevation_m,
    procedural_unsheltered_wind_velocity_m_s,
};
use rapier2d::prelude::Vector;

const MATCH_SEED: u64 = 0x5eed;
const TICK: u64 = 12_345;
const WORLD_WIDTH_M: f32 = 2_048.0;
const WORLD_HEIGHT_M: f32 = 1_536.0;
const COLUMNS: usize = 120;
const ROWS: usize = 90;
const WATER_COLOR_LEVELS: usize = 32;
const PLOT_X: f32 = 60.0;
const PLOT_Y: f32 = 90.0;
const PLOT_WIDTH: f32 = 960.0;
const PLOT_HEIGHT: f32 = 720.0;

fn representative_coast_center() -> Vector {
    let ring = [
        Vector::new(768.0, 0.0),
        Vector::new(543.0, 543.0),
        Vector::new(0.0, 768.0),
        Vector::new(-543.0, 543.0),
        Vector::new(-768.0, 0.0),
        Vector::new(-543.0, -543.0),
        Vector::new(0.0, -768.0),
        Vector::new(543.0, -543.0),
    ];
    let mut center = Vector::ZERO;
    let mut best_score = f32::MIN;
    for x in -64..=64 {
        for y in -64..=64 {
            let candidate = Vector::new(x as f32 * 128.0, y as f32 * 128.0);
            let elevation_m = procedural_terrain_elevation_m(MATCH_SEED, candidate);
            if elevation_m < 0.4 {
                continue;
            }
            let surrounding_water = ring
                .iter()
                .filter(|offset| {
                    procedural_terrain_elevation_m(MATCH_SEED, candidate + **offset) < 0.0
                })
                .count();
            let score = elevation_m + surrounding_water as f32;
            if score > best_score {
                best_score = score;
                center = candidate;
            }
        }
    }
    center
}

fn world_position(center: Vector, column: usize, row: usize) -> Vector {
    Vector::new(
        center.x + (column as f32 + 0.5) / COLUMNS as f32 * WORLD_WIDTH_M - WORLD_WIDTH_M / 2.0,
        center.y + WORLD_HEIGHT_M / 2.0 - (row as f32 + 0.5) / ROWS as f32 * WORLD_HEIGHT_M,
    )
}

fn water_color(retained_wind: f32) -> String {
    let stops: [(f32, [f32; 3]); 4] = [
        (0.3, [20.0, 45.0, 90.0]),
        (0.5, [28.0, 119.0, 166.0]),
        (0.75, [105.0, 196.0, 218.0]),
        (1.0, [226.0, 245.0, 247.0]),
    ];
    let retained_wind = retained_wind.clamp(0.3, 1.0);
    let (lower, upper) = stops
        .windows(2)
        .find_map(|pair| (retained_wind <= pair[1].0).then_some((pair[0], pair[1])))
        .unwrap();
    let amount = (retained_wind - lower.0) / (upper.0 - lower.0);
    let channel = |index| (lower.1[index] + amount * (upper.1[index] - lower.1[index])) as u8;
    format!("#{:02x}{:02x}{:02x}", channel(0), channel(1), channel(2))
}

fn water_color_level(retained_wind: f32) -> usize {
    (((retained_wind.clamp(0.3, 1.0) - 0.3) / 0.7 * (WATER_COLOR_LEVELS - 1) as f32).round())
        as usize
}

fn land_color_level(elevation_m: f32) -> usize {
    if elevation_m < 0.4 {
        0
    } else if elevation_m < 1.5 {
        1
    } else {
        2
    }
}

fn main() {
    let output_path = env::args()
        .nth(1)
        .unwrap_or_else(|| "docs/wind-shelter.svg".to_owned());
    let center = representative_coast_center();
    let cell_width = PLOT_WIDTH / COLUMNS as f32;
    let cell_height = PLOT_HEIGHT / ROWS as f32;
    let mut colors = (0..WATER_COLOR_LEVELS)
        .map(|level| water_color(0.3 + 0.7 * level as f32 / (WATER_COLOR_LEVELS - 1) as f32))
        .collect::<Vec<_>>();
    colors.extend(["#d8c28b", "#6f9b54", "#486f3c"].map(str::to_owned));
    let mut color_paths = vec![String::new(); colors.len()];
    let mut svg = String::new();
    write!(
        svg,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="1240" height="880" viewBox="0 0 1240 880">
<defs>
  <marker id="arrow" markerWidth="7" markerHeight="7" refX="6" refY="3.5" orient="auto"><path d="M0,0 L7,3.5 L0,7 Z" fill="#ffffff" stroke="#14202b" stroke-width="0.7"/></marker>
  <linearGradient id="shelter" x1="0" y1="1" x2="0" y2="0"><stop offset="0" stop-color="#142d5a"/><stop offset="0.286" stop-color="#1c77a6"/><stop offset="0.643" stop-color="#69c4da"/><stop offset="1" stop-color="#e2f5f7"/></linearGradient>
</defs>
<rect width="1240" height="880" fill="#f5f2ea"/>
<text x="60" y="38" font-family="sans-serif" font-size="25" font-weight="700" fill="#17202a">Land shelter in the seeded surface wind</text>
<text x="60" y="65" font-family="sans-serif" font-size="14" fill="#4a5560">Color isolates the island's effect · arrows show actual air motion and strength</text>
"##
    )
    .unwrap();

    for row in 0..ROWS {
        for column in 0..COLUMNS {
            let position_m = world_position(center, column, row);
            let elevation_m = procedural_terrain_elevation_m(MATCH_SEED, position_m);
            let color_level = if elevation_m >= 0.0 {
                WATER_COLOR_LEVELS + land_color_level(elevation_m)
            } else {
                let open_speed_m_s =
                    procedural_unsheltered_wind_velocity_m_s(MATCH_SEED, TICK, position_m).length();
                let local_speed_m_s = procedural_match_environment(MATCH_SEED, TICK, position_m)
                    .true_wind_velocity_m_s
                    .length();
                let retained_wind = if open_speed_m_s == 0.0 {
                    1.0
                } else {
                    local_speed_m_s / open_speed_m_s
                };
                water_color_level(retained_wind)
            };
            write!(
                color_paths[color_level],
                "M{:.1},{:.1}h{:.1}v{:.1}h-{:.1}z",
                PLOT_X + column as f32 * cell_width,
                PLOT_Y + row as f32 * cell_height,
                cell_width + 0.2,
                cell_height + 0.2,
                cell_width + 0.2
            )
            .unwrap();
        }
    }
    for (color, path) in colors.iter().zip(color_paths) {
        writeln!(svg, r#"<path fill="{color}" d="{path}"/>"#).unwrap();
    }

    for row in (4..ROWS).step_by(9) {
        for column in (4..COLUMNS).step_by(9) {
            let position_m = world_position(center, column, row);
            if procedural_terrain_elevation_m(MATCH_SEED, position_m) >= 0.0 {
                continue;
            }
            let wind =
                procedural_match_environment(MATCH_SEED, TICK, position_m).true_wind_velocity_m_s;
            let speed_m_s = wind.length();
            if speed_m_s == 0.0 {
                continue;
            }
            let direction = wind / speed_m_s;
            let length = (6.0 + 2.0 * speed_m_s).min(42.0);
            let x = PLOT_X + (column as f32 + 0.5) * cell_width;
            let y = PLOT_Y + (row as f32 + 0.5) * cell_height;
            let dx = direction.x * length;
            let dy = -direction.y * length;
            writeln!(
                svg,
                r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="#14202b" stroke-opacity="0.55" stroke-width="4"/><line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="#ffffff" stroke-width="1.8" marker-end="url(#arrow)"/>"##,
                x - 0.4 * dx,
                y - 0.4 * dy,
                x + 0.6 * dx,
                y + 0.6 * dy,
                x - 0.4 * dx,
                y - 0.4 * dy,
                x + 0.6 * dx,
                y + 0.6 * dy
            )
            .unwrap();
        }
    }

    let scale_width = 500.0 / WORLD_WIDTH_M * PLOT_WIDTH;
    write!(
        svg,
        r##"<rect x="{PLOT_X}" y="{PLOT_Y}" width="{PLOT_WIDTH}" height="{PLOT_HEIGHT}" fill="none" stroke="#17202a" stroke-width="2"/>
<line x1="80" y1="790" x2="{:.1}" y2="790" stroke="#ffffff" stroke-width="6"/><line x1="80" y1="790" x2="{:.1}" y2="790" stroke="#17202a" stroke-width="2"/><text x="80" y="814" font-family="sans-serif" font-size="13" fill="#17202a">500 m</text>
<text x="1060" y="112" font-family="sans-serif" font-size="16" font-weight="700" fill="#17202a">Wind retained</text>
<rect x="1060" y="135" width="30" height="360" fill="url(#shelter)" stroke="#17202a"/>
<text x="1100" y="143" font-family="sans-serif" font-size="13" fill="#17202a">100%</text><text x="1100" y="263" font-family="sans-serif" font-size="13" fill="#17202a">75%</text><text x="1100" y="383" font-family="sans-serif" font-size="13" fill="#17202a">50%</text><text x="1100" y="495" font-family="sans-serif" font-size="13" fill="#17202a">30%</text>
<rect x="1060" y="540" width="22" height="22" fill="#d8c28b"/><text x="1092" y="556" font-family="sans-serif" font-size="13" fill="#17202a">Beach</text>
<rect x="1060" y="574" width="22" height="22" fill="#6f9b54"/><text x="1092" y="590" font-family="sans-serif" font-size="13" fill="#17202a">Land</text>
<rect x="1060" y="608" width="22" height="22" fill="#486f3c"/><text x="1092" y="624" font-family="sans-serif" font-size="13" fill="#17202a">High land</text>
<text x="1060" y="680" font-family="sans-serif" font-size="13" fill="#4a5560"><tspan x="1060" dy="0">Dark water and shorter</tspan><tspan x="1060" dy="19">arrows reveal the lee</tspan><tspan x="1060" dy="19">wind shadow.</tspan></text>
<text x="60" y="852" font-family="sans-serif" font-size="12" fill="#66717b">View center: ({:.0} m, {:.0} m). The model samples upwind terrain out to 512 m; shelter changes speed but not direction.</text>
</svg>
"##,
        80.0 + scale_width,
        80.0 + scale_width,
        center.x,
        center.y
    )
    .unwrap();

    fs::write(&output_path, svg).unwrap();
    println!("wrote {output_path}");
}
