use crate::text::font::{CellMetrics, GlyphBitmap, GlyphContent};

const NONE: u8 = 0;
const LIGHT: u8 = 1;
const HEAVY: u8 = 2;
const DOUBLE: u8 = 3;

const LIGHT_SHADE: u8 = 0x40;
const MEDIUM_SHADE: u8 = 0x80;
const DARK_SHADE: u8 = 0xc0;
const OPAQUE: u8 = 0xff;

const SUPERSAMPLES: u32 = 4;

pub fn is_sprite(character: char) -> bool {
    matches!(
        character,
        '\u{2500}'..='\u{259f}' | '\u{e0b0}'..='\u{e0b3}' | '\u{f5d0}'..='\u{f60d}'
    )
}

pub fn draw(character: char, metrics: CellMetrics, budget: u32) -> Option<GlyphBitmap> {
    if !is_sprite(character) {
        return None;
    }
    let mut canvas = Canvas::new(metrics.width * budget.max(1), metrics.height);
    let light = metrics.stroke.max(1) as i32;
    let pen = Pen {
        light,
        heavy: light * 2,
    };
    match character {
        '\u{2580}'..='\u{259f}' => block(&mut canvas, character),
        '\u{2504}'..='\u{250b}' | '\u{254c}'..='\u{254f}' => dashes(&mut canvas, &pen, character),
        '\u{256d}'..='\u{2570}' => arc(&mut canvas, &pen, character),
        '\u{2571}'..='\u{2573}' => diagonals(&mut canvas, &pen, character),
        '\u{2500}'..='\u{257f}' => lines(&mut canvas, &pen, arms_of(character)?),
        '\u{e0b0}'..='\u{e0b3}' => powerline(&mut canvas, &pen, character),
        '\u{f5d0}'..='\u{f60d}' => branch(&mut canvas, &pen, character),
        _ => return None,
    }
    Some(GlyphBitmap {
        left: 0,
        top: metrics.baseline as i32,
        width: canvas.width as u32,
        height: canvas.height as u32,
        coverage: canvas.coverage,
        content: GlyphContent::Mask,
    })
}

struct Canvas {
    width: i32,
    height: i32,
    coverage: Vec<u8>,
}

impl Canvas {
    fn new(width: u32, height: u32) -> Self {
        let width = width.max(1) as i32;
        let height = height.max(1) as i32;
        Self {
            width,
            height,
            coverage: vec![0; (width * height) as usize],
        }
    }

    fn fill(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, alpha: u8) {
        let (x0, x1) = (x0.clamp(0, self.width), x1.clamp(0, self.width));
        let (y0, y1) = (y0.clamp(0, self.height), y1.clamp(0, self.height));
        for y in y0..y1 {
            for x in x0..x1 {
                self.blend(x, y, alpha);
            }
        }
    }

    fn blend(&mut self, x: i32, y: i32, alpha: u8) {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return;
        }
        let index = (y * self.width + x) as usize;
        self.coverage[index] = self.coverage[index].max(alpha);
    }

    fn shade(&mut self, inside: impl Fn(f32, f32) -> bool) {
        let step = 1.0 / SUPERSAMPLES as f32;
        let total = (SUPERSAMPLES * SUPERSAMPLES) as u32;
        for y in 0..self.height {
            for x in 0..self.width {
                let mut hits = 0u32;
                for sy in 0..SUPERSAMPLES {
                    for sx in 0..SUPERSAMPLES {
                        let px = x as f32 + (sx as f32 + 0.5) * step;
                        let py = y as f32 + (sy as f32 + 0.5) * step;
                        if inside(px, py) {
                            hits += 1;
                        }
                    }
                }
                if hits > 0 {
                    self.blend(x, y, ((hits * 255 + total / 2) / total) as u8);
                }
            }
        }
    }
}

struct Pen {
    light: i32,
    heavy: i32,
}

impl Pen {
    fn thickness(&self, weight: u8) -> i32 {
        match weight {
            HEAVY => self.heavy,
            DOUBLE => self.light * 3,
            _ => self.light,
        }
    }
}

fn centered(extent: i32, thickness: i32) -> (i32, i32) {
    let start = (extent - thickness) / 2;
    (start, start + thickness)
}

fn eighths(extent: i32, count: i32) -> i32 {
    (extent * count + 4) / 8
}

fn block(canvas: &mut Canvas, character: char) {
    let (w, h) = (canvas.width, canvas.height);
    let lower = |count: i32| h - eighths(h, count);
    let left = |count: i32| eighths(w, count);
    let mid_x = left(4);
    let mid_y = lower(4);
    let quadrants = |canvas: &mut Canvas,
                     upper_left: bool,
                     upper_right: bool,
                     lower_left: bool,
                     lower_right: bool| {
        if upper_left {
            canvas.fill(0, 0, mid_x, mid_y, OPAQUE);
        }
        if upper_right {
            canvas.fill(mid_x, 0, w, mid_y, OPAQUE);
        }
        if lower_left {
            canvas.fill(0, mid_y, mid_x, h, OPAQUE);
        }
        if lower_right {
            canvas.fill(mid_x, mid_y, w, h, OPAQUE);
        }
    };
    match character {
        '\u{2580}' => canvas.fill(0, 0, w, mid_y, OPAQUE),
        '\u{2581}'..='\u{2588}' => {
            let count = character as i32 - 0x2580;
            canvas.fill(0, lower(count), w, h, OPAQUE);
        },
        '\u{2589}'..='\u{258f}' => {
            let count = 0x2590 - character as i32;
            canvas.fill(0, 0, left(count), h, OPAQUE);
        },
        '\u{2590}' => canvas.fill(mid_x, 0, w, h, OPAQUE),
        '\u{2591}' => canvas.fill(0, 0, w, h, LIGHT_SHADE),
        '\u{2592}' => canvas.fill(0, 0, w, h, MEDIUM_SHADE),
        '\u{2593}' => canvas.fill(0, 0, w, h, DARK_SHADE),
        '\u{2594}' => canvas.fill(0, 0, w, h - lower(1), OPAQUE),
        '\u{2595}' => canvas.fill(w - left(1), 0, w, h, OPAQUE),
        '\u{2596}' => quadrants(canvas, false, false, true, false),
        '\u{2597}' => quadrants(canvas, false, false, false, true),
        '\u{2598}' => quadrants(canvas, true, false, false, false),
        '\u{2599}' => quadrants(canvas, true, false, true, true),
        '\u{259a}' => quadrants(canvas, true, false, false, true),
        '\u{259b}' => quadrants(canvas, true, true, true, false),
        '\u{259c}' => quadrants(canvas, true, true, false, true),
        '\u{259d}' => quadrants(canvas, false, true, false, false),
        '\u{259e}' => quadrants(canvas, false, true, true, false),
        '\u{259f}' => quadrants(canvas, false, true, true, true),
        _ => {},
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Arms {
    up: u8,
    right: u8,
    down: u8,
    left: u8,
}

const fn arms(up: u8, right: u8, down: u8, left: u8) -> Arms {
    Arms {
        up,
        right,
        down,
        left,
    }
}

fn arms_of(character: char) -> Option<Arms> {
    const L: u8 = LIGHT;
    const H: u8 = HEAVY;
    const D: u8 = DOUBLE;
    const O: u8 = NONE;
    let arms = match character {
        '\u{2500}' => arms(O, L, O, L),
        '\u{2501}' => arms(O, H, O, H),
        '\u{2502}' => arms(L, O, L, O),
        '\u{2503}' => arms(H, O, H, O),
        '\u{250c}' => arms(O, L, L, O),
        '\u{250d}' => arms(O, H, L, O),
        '\u{250e}' => arms(O, L, H, O),
        '\u{250f}' => arms(O, H, H, O),
        '\u{2510}' => arms(O, O, L, L),
        '\u{2511}' => arms(O, O, L, H),
        '\u{2512}' => arms(O, O, H, L),
        '\u{2513}' => arms(O, O, H, H),
        '\u{2514}' => arms(L, L, O, O),
        '\u{2515}' => arms(L, H, O, O),
        '\u{2516}' => arms(H, L, O, O),
        '\u{2517}' => arms(H, H, O, O),
        '\u{2518}' => arms(L, O, O, L),
        '\u{2519}' => arms(L, O, O, H),
        '\u{251a}' => arms(H, O, O, L),
        '\u{251b}' => arms(H, O, O, H),
        '\u{251c}' => arms(L, L, L, O),
        '\u{251d}' => arms(L, H, L, O),
        '\u{251e}' => arms(H, L, L, O),
        '\u{251f}' => arms(L, L, H, O),
        '\u{2520}' => arms(H, L, H, O),
        '\u{2521}' => arms(H, H, L, O),
        '\u{2522}' => arms(L, H, H, O),
        '\u{2523}' => arms(H, H, H, O),
        '\u{2524}' => arms(L, O, L, L),
        '\u{2525}' => arms(L, O, L, H),
        '\u{2526}' => arms(H, O, L, L),
        '\u{2527}' => arms(L, O, H, L),
        '\u{2528}' => arms(H, O, H, L),
        '\u{2529}' => arms(H, O, L, H),
        '\u{252a}' => arms(L, O, H, H),
        '\u{252b}' => arms(H, O, H, H),
        '\u{252c}' => arms(O, L, L, L),
        '\u{252d}' => arms(O, L, L, H),
        '\u{252e}' => arms(O, H, L, L),
        '\u{252f}' => arms(O, H, L, H),
        '\u{2530}' => arms(O, L, H, L),
        '\u{2531}' => arms(O, L, H, H),
        '\u{2532}' => arms(O, H, H, L),
        '\u{2533}' => arms(O, H, H, H),
        '\u{2534}' => arms(L, L, O, L),
        '\u{2535}' => arms(L, L, O, H),
        '\u{2536}' => arms(L, H, O, L),
        '\u{2537}' => arms(L, H, O, H),
        '\u{2538}' => arms(H, L, O, L),
        '\u{2539}' => arms(H, L, O, H),
        '\u{253a}' => arms(H, H, O, L),
        '\u{253b}' => arms(H, H, O, H),
        '\u{253c}' => arms(L, L, L, L),
        '\u{253d}' => arms(L, L, L, H),
        '\u{253e}' => arms(L, H, L, L),
        '\u{253f}' => arms(L, H, L, H),
        '\u{2540}' => arms(H, L, L, L),
        '\u{2541}' => arms(L, L, H, L),
        '\u{2542}' => arms(H, L, H, L),
        '\u{2543}' => arms(H, L, L, H),
        '\u{2544}' => arms(H, H, L, L),
        '\u{2545}' => arms(L, L, H, H),
        '\u{2546}' => arms(L, H, H, L),
        '\u{2547}' => arms(H, H, L, H),
        '\u{2548}' => arms(L, H, H, H),
        '\u{2549}' => arms(H, L, H, H),
        '\u{254a}' => arms(H, H, H, L),
        '\u{254b}' => arms(H, H, H, H),
        '\u{2550}' => arms(O, D, O, D),
        '\u{2551}' => arms(D, O, D, O),
        '\u{2552}' => arms(O, D, L, O),
        '\u{2553}' => arms(O, L, D, O),
        '\u{2554}' => arms(O, D, D, O),
        '\u{2555}' => arms(O, O, L, D),
        '\u{2556}' => arms(O, O, D, L),
        '\u{2557}' => arms(O, O, D, D),
        '\u{2558}' => arms(L, D, O, O),
        '\u{2559}' => arms(D, L, O, O),
        '\u{255a}' => arms(D, D, O, O),
        '\u{255b}' => arms(L, O, O, D),
        '\u{255c}' => arms(D, O, O, L),
        '\u{255d}' => arms(D, O, O, D),
        '\u{255e}' => arms(L, D, L, O),
        '\u{255f}' => arms(D, L, D, O),
        '\u{2560}' => arms(D, D, D, O),
        '\u{2561}' => arms(L, O, L, D),
        '\u{2562}' => arms(D, O, D, L),
        '\u{2563}' => arms(D, O, D, D),
        '\u{2564}' => arms(O, D, L, D),
        '\u{2565}' => arms(O, L, D, L),
        '\u{2566}' => arms(O, D, D, D),
        '\u{2567}' => arms(L, D, O, D),
        '\u{2568}' => arms(D, L, O, L),
        '\u{2569}' => arms(D, D, O, D),
        '\u{256a}' => arms(L, D, L, D),
        '\u{256b}' => arms(D, L, D, L),
        '\u{256c}' => arms(D, D, D, D),
        '\u{2574}' => arms(O, O, O, L),
        '\u{2575}' => arms(L, O, O, O),
        '\u{2576}' => arms(O, L, O, O),
        '\u{2577}' => arms(O, O, L, O),
        '\u{2578}' => arms(O, O, O, H),
        '\u{2579}' => arms(H, O, O, O),
        '\u{257a}' => arms(O, H, O, O),
        '\u{257b}' => arms(O, O, H, O),
        '\u{257c}' => arms(O, H, O, L),
        '\u{257d}' => arms(L, O, H, O),
        '\u{257e}' => arms(O, L, O, H),
        '\u{257f}' => arms(H, O, L, O),
        _ => return None,
    };
    Some(arms)
}

struct Pair {
    first: (i32, i32),
    second: (i32, i32),
}

fn pair(extent: i32, light: i32) -> Pair {
    let (start, _) = centered(extent, light * 3);
    Pair {
        first: (start, start + light),
        second: (start + light * 2, start + light * 3),
    }
}

fn thickest_single(pen: &Pen, a: u8, b: u8) -> i32 {
    [a, b]
        .into_iter()
        .filter(|weight| *weight == LIGHT || *weight == HEAVY)
        .map(|weight| pen.thickness(weight))
        .max()
        .unwrap_or(0)
}

fn crossing(extent: i32, pen: &Pen, a: u8, b: u8) -> Option<Pair> {
    if a == DOUBLE || b == DOUBLE {
        return Some(pair(extent, pen.light));
    }
    let thickness = thickest_single(pen, a, b);
    if thickness == 0 {
        return None;
    }
    let span = centered(extent, thickness);
    Some(Pair {
        first: span,
        second: span,
    })
}

fn lines(canvas: &mut Canvas, pen: &Pen, arms: Arms) {
    let (w, h) = (canvas.width, canvas.height);
    let vertical_double = arms.up == DOUBLE || arms.down == DOUBLE;
    let horizontal_double = arms.left == DOUBLE || arms.right == DOUBLE;
    let columns = crossing(w, pen, arms.up, arms.down);
    let rows = crossing(h, pen, arms.left, arms.right);

    for (weight, is_left) in [(arms.left, true), (arms.right, false)] {
        match weight {
            NONE => {},
            DOUBLE => {
                let own = pair(h, pen.light);
                let reach = |toward_up: bool| -> i32 {
                    let (near, far) = if toward_up {
                        (arms.up, arms.down)
                    } else {
                        (arms.down, arms.up)
                    };
                    match &columns {
                        Some(columns) if near != NONE => {
                            if is_left {
                                columns.first.1
                            } else {
                                columns.second.0
                            }
                        },
                        Some(columns) if far != NONE => {
                            if is_left {
                                columns.second.1
                            } else {
                                columns.first.0
                            }
                        },
                        _ => w / 2,
                    }
                };
                for (span, toward_up) in [(own.first, true), (own.second, false)] {
                    let edge = reach(toward_up);
                    if is_left {
                        canvas.fill(0, span.0, edge, span.1, OPAQUE);
                    } else {
                        canvas.fill(edge, span.0, w, span.1, OPAQUE);
                    }
                }
            },
            _ => {
                let span = centered(h, pen.thickness(weight));
                let opposite = if is_left { arms.right } else { arms.left };
                let junction = centered(w, junction_thickness(pen, arms.up, arms.down, weight));
                if is_left {
                    let end = match &columns {
                        Some(columns) if vertical_double && opposite == NONE => columns.first.1,
                        _ => junction.1,
                    };
                    canvas.fill(0, span.0, end, span.1, OPAQUE);
                } else {
                    let start = match &columns {
                        Some(columns) if vertical_double && opposite == NONE => columns.second.0,
                        _ => junction.0,
                    };
                    canvas.fill(start, span.0, w, span.1, OPAQUE);
                }
            },
        }
    }

    for (weight, is_up) in [(arms.up, true), (arms.down, false)] {
        match weight {
            NONE => {},
            DOUBLE => {
                let own = pair(w, pen.light);
                let reach = |toward_left: bool| -> i32 {
                    let (near, far) = if toward_left {
                        (arms.left, arms.right)
                    } else {
                        (arms.right, arms.left)
                    };
                    match &rows {
                        Some(rows) if near != NONE => {
                            if is_up {
                                rows.first.1
                            } else {
                                rows.second.0
                            }
                        },
                        Some(rows) if far != NONE => {
                            if is_up {
                                rows.second.1
                            } else {
                                rows.first.0
                            }
                        },
                        _ => h / 2,
                    }
                };
                for (span, toward_left) in [(own.first, true), (own.second, false)] {
                    let edge = reach(toward_left);
                    if is_up {
                        canvas.fill(span.0, 0, span.1, edge, OPAQUE);
                    } else {
                        canvas.fill(span.0, edge, span.1, h, OPAQUE);
                    }
                }
            },
            _ => {
                let span = centered(w, pen.thickness(weight));
                let opposite = if is_up { arms.down } else { arms.up };
                let junction = centered(h, junction_thickness(pen, arms.left, arms.right, weight));
                if is_up {
                    let end = match &rows {
                        Some(rows) if horizontal_double && opposite == NONE => rows.first.1,
                        _ => junction.1,
                    };
                    canvas.fill(span.0, 0, span.1, end, OPAQUE);
                } else {
                    let start = match &rows {
                        Some(rows) if horizontal_double && opposite == NONE => rows.second.0,
                        _ => junction.0,
                    };
                    canvas.fill(span.0, start, span.1, h, OPAQUE);
                }
            },
        }
    }
}

fn junction_thickness(pen: &Pen, a: u8, b: u8, own: u8) -> i32 {
    match thickest_single(pen, a, b) {
        0 => pen.thickness(own),
        thickness => thickness,
    }
}

fn dashes(canvas: &mut Canvas, pen: &Pen, character: char) {
    let (count, weight, horizontal) = match character {
        '\u{2504}' => (3, LIGHT, true),
        '\u{2505}' => (3, HEAVY, true),
        '\u{2506}' => (3, LIGHT, false),
        '\u{2507}' => (3, HEAVY, false),
        '\u{2508}' => (4, LIGHT, true),
        '\u{2509}' => (4, HEAVY, true),
        '\u{250a}' => (4, LIGHT, false),
        '\u{250b}' => (4, HEAVY, false),
        '\u{254c}' => (2, LIGHT, true),
        '\u{254d}' => (2, HEAVY, true),
        '\u{254e}' => (2, LIGHT, false),
        _ => (2, HEAVY, false),
    };
    let thickness = pen.thickness(weight);
    let (w, h) = (canvas.width, canvas.height);
    let length = if horizontal { w } else { h };
    let gap = (length / (count * 4)).max(1);
    for index in 0..count {
        let start = length * index / count;
        let end = length * (index + 1) / count;
        let (from, to) = (start + gap / 2, end - (gap - gap / 2));
        if to <= from {
            continue;
        }
        if horizontal {
            let span = centered(h, thickness);
            canvas.fill(from, span.0, to, span.1, OPAQUE);
        } else {
            let span = centered(w, thickness);
            canvas.fill(span.0, from, span.1, to, OPAQUE);
        }
    }
}

fn arc(canvas: &mut Canvas, pen: &Pen, character: char) {
    let corner = match character {
        '\u{256d}' => DOWN_RIGHT,
        '\u{256e}' => DOWN_LEFT,
        '\u{256f}' => UP_LEFT,
        _ => UP_RIGHT,
    };
    rounded_corner(canvas, pen, corner);
}

/// A rounded corner, given as the two cell edges that it joins.
#[derive(Clone, Copy)]
struct Corner {
    toward_right: bool,
    toward_down: bool,
}

/// `╭`
const DOWN_RIGHT: Corner = Corner {
    toward_right: true,
    toward_down: true,
};
/// `╮`
const DOWN_LEFT: Corner = Corner {
    toward_right: false,
    toward_down: true,
};
/// `╯`
const UP_LEFT: Corner = Corner {
    toward_right: false,
    toward_down: false,
};
/// `╰`
const UP_RIGHT: Corner = Corner {
    toward_right: true,
    toward_down: false,
};

fn rounded_corner(canvas: &mut Canvas, pen: &Pen, corner: Corner) {
    let (w, h) = (canvas.width, canvas.height);
    let thickness = pen.light;
    let columns = centered(w, thickness);
    let rows = centered(h, thickness);
    let Corner {
        toward_right,
        toward_down,
    } = corner;
    let line_x = (columns.0 + columns.1) as f32 / 2.0;
    let line_y = (rows.0 + rows.1) as f32 / 2.0;
    let room_x = if toward_right {
        w as f32 - line_x
    } else {
        line_x
    };
    let room_y = if toward_down {
        h as f32 - line_y
    } else {
        line_y
    };
    let radius = (room_x.min(room_y) - 1.0).max(thickness as f32);
    let sign_x = if toward_right { 1.0 } else { -1.0 };
    let sign_y = if toward_down { 1.0 } else { -1.0 };
    let centre_x = line_x + sign_x * radius;
    let centre_y = line_y + sign_y * radius;
    let half = thickness as f32 / 2.0;

    canvas.shade(|x, y| {
        let beyond_x = (x - centre_x) * sign_x > 0.0;
        let beyond_y = (y - centre_y) * sign_y > 0.0;
        if beyond_x || beyond_y {
            return false;
        }
        let distance = ((x - centre_x).powi(2) + (y - centre_y).powi(2)).sqrt();
        (distance - radius).abs() <= half
    });

    let arc_end_x = centre_x.round() as i32;
    if toward_right {
        canvas.fill(arc_end_x, rows.0, w, rows.1, OPAQUE);
    } else {
        canvas.fill(0, rows.0, arc_end_x, rows.1, OPAQUE);
    }
    let arc_end_y = centre_y.round() as i32;
    if toward_down {
        canvas.fill(columns.0, arc_end_y, columns.1, h, OPAQUE);
    } else {
        canvas.fill(columns.0, 0, columns.1, arc_end_y, OPAQUE);
    }
}

/// The branch drawing symbols at U+F5D0 to U+F60D, which Kitty and Ghostty
/// also draw. Git graph tools use them to draw commit graphs: lines, fading
/// lines, rounded forks and merges, and commit dots with stubs that join the
/// lines around them.
fn branch(canvas: &mut Canvas, pen: &Pen, character: char) {
    const HORIZONTAL: Arms = arms(NONE, LIGHT, NONE, LIGHT);
    const VERTICAL: Arms = arms(LIGHT, NONE, LIGHT, NONE);
    // The straight line and the rounded corners of U+F5D6 to U+F5ED.
    let (straight, corners): (Option<Arms>, &[Corner]) = match character {
        '\u{f5d0}' => (Some(HORIZONTAL), &[]),
        '\u{f5d1}' => (Some(VERTICAL), &[]),
        '\u{f5d2}'..='\u{f5d5}' => return fading_line(canvas, pen, character),
        '\u{f5d6}' => (None, &[DOWN_RIGHT]),
        '\u{f5d7}' => (None, &[DOWN_LEFT]),
        '\u{f5d8}' => (None, &[UP_RIGHT]),
        '\u{f5d9}' => (None, &[UP_LEFT]),
        '\u{f5da}' => (Some(VERTICAL), &[UP_RIGHT]),
        '\u{f5db}' => (Some(VERTICAL), &[DOWN_RIGHT]),
        '\u{f5dc}' => (None, &[UP_RIGHT, DOWN_RIGHT]),
        '\u{f5dd}' => (Some(VERTICAL), &[UP_LEFT]),
        '\u{f5de}' => (Some(VERTICAL), &[DOWN_LEFT]),
        '\u{f5df}' => (None, &[UP_LEFT, DOWN_LEFT]),
        '\u{f5e0}' => (Some(HORIZONTAL), &[DOWN_LEFT]),
        '\u{f5e1}' => (Some(HORIZONTAL), &[DOWN_RIGHT]),
        '\u{f5e2}' => (None, &[DOWN_RIGHT, DOWN_LEFT]),
        '\u{f5e3}' => (Some(HORIZONTAL), &[UP_LEFT]),
        '\u{f5e4}' => (Some(HORIZONTAL), &[UP_RIGHT]),
        '\u{f5e5}' => (None, &[UP_RIGHT, UP_LEFT]),
        '\u{f5e6}' => (Some(VERTICAL), &[UP_LEFT, UP_RIGHT]),
        '\u{f5e7}' => (Some(VERTICAL), &[DOWN_LEFT, DOWN_RIGHT]),
        '\u{f5e8}' => (Some(HORIZONTAL), &[DOWN_LEFT, UP_LEFT]),
        '\u{f5e9}' => (Some(HORIZONTAL), &[UP_RIGHT, DOWN_RIGHT]),
        '\u{f5ea}' => (Some(VERTICAL), &[UP_LEFT, DOWN_RIGHT]),
        '\u{f5eb}' => (Some(VERTICAL), &[UP_RIGHT, DOWN_LEFT]),
        '\u{f5ec}' => (Some(HORIZONTAL), &[UP_LEFT, DOWN_RIGHT]),
        '\u{f5ed}' => (Some(HORIZONTAL), &[UP_RIGHT, DOWN_LEFT]),
        _ => return commit_dot(canvas, pen, character),
    };
    if let Some(straight) = straight {
        lines(canvas, pen, straight);
    }
    for corner in corners {
        rounded_corner(canvas, pen, *corner);
    }
}

/// A line that fades out toward one edge of the cell, U+F5D2 to U+F5D5.
fn fading_line(canvas: &mut Canvas, pen: &Pen, character: char) {
    let (w, h) = (canvas.width, canvas.height);
    let thickness = pen.light;
    // The fraction of the way from the opaque edge to the faded edge.
    let alpha = |step: i32, length: i32| -> u8 {
        let fraction = (step as f32 + 0.5) / length as f32;
        ((1.0 - fraction) * OPAQUE as f32).round() as u8
    };
    match character {
        // Fades toward the right or the left.
        '\u{f5d2}' | '\u{f5d3}' => {
            let rows = centered(h, thickness);
            for x in 0..w {
                let step = if character == '\u{f5d2}' {
                    x
                } else {
                    w - 1 - x
                };
                canvas.fill(x, rows.0, x + 1, rows.1, alpha(step, w));
            }
        },
        // Fades toward the bottom or the top.
        _ => {
            let columns = centered(w, thickness);
            for y in 0..h {
                let step = if character == '\u{f5d4}' {
                    y
                } else {
                    h - 1 - y
                };
                canvas.fill(columns.0, y, columns.1, y + 1, alpha(step, h));
            }
        },
    }
}

/// A commit dot, U+F5EE to U+F60D, with stubs that join the lines at some
/// of the cell edges. Each stub set comes as a filled dot and then an empty
/// dot.
fn commit_dot(canvas: &mut Canvas, pen: &Pen, character: char) {
    // Up, right, down, and left stubs, in the order of the code points.
    const STUBS: [(bool, bool, bool, bool); 16] = [
        (false, false, false, false),
        (false, true, false, false),
        (false, false, false, true),
        (false, true, false, true),
        (false, false, true, false),
        (true, false, false, false),
        (true, false, true, false),
        (false, true, true, false),
        (false, false, true, true),
        (true, true, false, false),
        (true, false, false, true),
        (true, true, true, false),
        (true, false, true, true),
        (false, true, true, true),
        (true, true, false, true),
        (true, true, true, true),
    ];
    let offset = character as usize - 0xf5ee;
    let (up, right, down, left) = STUBS[offset / 2];
    let filled = offset % 2 == 0;

    let (w, h) = (canvas.width, canvas.height);
    let thickness = pen.light;
    let columns = centered(w, thickness);
    let rows = centered(h, thickness);
    // Put the centre on the lines, so that the stubs and the box drawing
    // lines meet the dot in the same place.
    let centre_x = (columns.0 + columns.1) as f32 / 2.0;
    let centre_y = (rows.0 + rows.1) as f32 / 2.0;
    let radius = centre_x
        .min(centre_y)
        .min(w as f32 - centre_x)
        .min(h as f32 - centre_y);
    let half = thickness as f32 / 2.0;

    if up {
        let end = (centre_y - radius + half).ceil() as i32;
        canvas.fill(columns.0, 0, columns.1, end, OPAQUE);
    }
    if down {
        let start = (centre_y + radius - half).floor() as i32;
        canvas.fill(columns.0, start, columns.1, h, OPAQUE);
    }
    if left {
        let end = (centre_x - radius + half).ceil() as i32;
        canvas.fill(0, rows.0, end, rows.1, OPAQUE);
    }
    if right {
        let start = (centre_x + radius - half).floor() as i32;
        canvas.fill(start, rows.0, w, rows.1, OPAQUE);
    }
    canvas.shade(|x, y| {
        let distance = ((x - centre_x).powi(2) + (y - centre_y).powi(2)).sqrt();
        if filled {
            distance <= radius
        } else {
            // A ring with the line thickness, inside the radius.
            (distance - (radius - half)).abs() <= half
        }
    });
}

fn diagonals(canvas: &mut Canvas, pen: &Pen, character: char) {
    let (w, h) = (canvas.width as f32, canvas.height as f32);
    let half = pen.light as f32 / 2.0;
    let length = (w * w + h * h).sqrt();
    let rising = matches!(character, '\u{2571}' | '\u{2573}');
    let falling = matches!(character, '\u{2572}' | '\u{2573}');
    canvas.shade(|x, y| {
        let near_rising = ((h * x + w * y - w * h) / length).abs() <= half;
        let near_falling = ((h * x - w * y) / length).abs() <= half;
        (rising && near_rising) || (falling && near_falling)
    });
}

fn powerline(canvas: &mut Canvas, pen: &Pen, character: char) {
    let (w, h) = (canvas.width as f32, canvas.height as f32);
    let mid = h / 2.0;
    let pointing_right = matches!(character, '\u{e0b0}' | '\u{e0b1}');
    let solid = matches!(character, '\u{e0b0}' | '\u{e0b2}');
    let half = pen.light as f32 / 2.0;
    let slope_length = (w * w + mid * mid).sqrt();
    canvas.shade(|x, y| {
        let depth = if pointing_right { x } else { w - x };
        let reach = w * (1.0 - (y - mid).abs() / mid);
        if solid {
            depth <= reach
        } else {
            let offset = (y - mid).abs();
            let distance = (mid * depth + w * offset - w * mid).abs() / slope_length;
            distance <= half
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metrics(width: u32, height: u32) -> CellMetrics {
        CellMetrics {
            width,
            height,
            baseline: height * 3 / 4,
            underline_top: height - 2,
            strikeout_top: height / 2,
            stroke: 1,
        }
    }

    fn sprite(character: char, metrics: CellMetrics) -> GlyphBitmap {
        draw(character, metrics, 1).unwrap()
    }

    fn at(bitmap: &GlyphBitmap, x: u32, y: u32) -> u8 {
        bitmap.coverage[(y * bitmap.width + x) as usize]
    }

    #[test]
    fn sprites_fill_exactly_one_cell() {
        let metrics = metrics(8, 20);
        for character in ('\u{2500}'..='\u{259f}')
            .chain('\u{e0b0}'..='\u{e0b3}')
            .chain('\u{f5d0}'..='\u{f60d}')
        {
            let bitmap = sprite(character, metrics);
            assert_eq!((bitmap.width, bitmap.height), (8, 20), "{:?}", character);
            assert_eq!(bitmap.left, 0);
            assert_eq!(bitmap.top, metrics.baseline as i32);
            assert!(
                bitmap.coverage.iter().any(|alpha| *alpha > 0),
                "{:?} drew nothing",
                character
            );
        }
    }

    #[test]
    fn a_full_block_covers_every_pixel_at_awkward_sizes() {
        for (width, height) in [(7, 17), (8, 20), (13, 31), (6, 15)] {
            let bitmap = sprite('\u{2588}', metrics(width, height));
            assert!(
                bitmap.coverage.iter().all(|alpha| *alpha == OPAQUE),
                "a {}x{} full block has gaps",
                width,
                height
            );
        }
    }

    #[test]
    fn partial_blocks_grow_from_the_left_with_hard_edges() {
        let metrics = metrics(8, 20);
        let mut previous = 0;
        for character in [
            '\u{258f}', '\u{258e}', '\u{258d}', '\u{258c}', '\u{258b}', '\u{258a}', '\u{2589}',
        ] {
            let bitmap = sprite(character, metrics);
            let filled = (0..bitmap.width)
                .filter(|x| at(&bitmap, *x, 10) == OPAQUE)
                .count();
            assert!(
                filled > previous,
                "{:?} is not wider than its predecessor",
                character
            );
            assert!(bitmap
                .coverage
                .iter()
                .all(|alpha| *alpha == 0 || *alpha == OPAQUE));
            assert_eq!(at(&bitmap, 0, 0), OPAQUE);
            previous = filled;
        }
    }

    #[test]
    fn lower_blocks_reach_the_bottom_edge() {
        let metrics = metrics(8, 20);
        for character in '\u{2581}'..='\u{2587}' {
            let bitmap = sprite(character, metrics);
            assert_eq!(at(&bitmap, 4, 19), OPAQUE, "{:?}", character);
            assert_eq!(at(&bitmap, 4, 0), 0, "{:?}", character);
        }
    }

    #[test]
    fn horizontal_lines_meet_their_neighbours_at_the_cell_edge() {
        let metrics = metrics(9, 21);
        for character in ['\u{2500}', '\u{2501}', '\u{2550}', '\u{253c}', '\u{256c}'] {
            let bitmap = sprite(character, metrics);
            let left: Vec<u8> = (0..bitmap.height).map(|y| at(&bitmap, 0, y)).collect();
            let right: Vec<u8> = (0..bitmap.height)
                .map(|y| at(&bitmap, bitmap.width - 1, y))
                .collect();
            assert_eq!(left, right, "{:?}", character);
            assert!(left.iter().any(|alpha| *alpha == OPAQUE));
        }
    }

    #[test]
    fn vertical_lines_meet_their_neighbours_at_the_cell_edge() {
        let metrics = metrics(9, 21);
        for character in ['\u{2502}', '\u{2503}', '\u{2551}', '\u{253c}', '\u{256c}'] {
            let bitmap = sprite(character, metrics);
            let top: Vec<u8> = (0..bitmap.width).map(|x| at(&bitmap, x, 0)).collect();
            let bottom: Vec<u8> = (0..bitmap.width)
                .map(|x| at(&bitmap, x, bitmap.height - 1))
                .collect();
            assert_eq!(top, bottom, "{:?}", character);
            assert!(top.iter().any(|alpha| *alpha == OPAQUE));
        }
    }

    #[test]
    fn box_lines_span_a_cell_adjusted_for_line_height_and_width() {
        use crate::text::font::{CellAdjust, FontOptions, FontStack};

        let metrics = FontStack::build(&FontOptions {
            system_fonts: false,
            cell: CellAdjust {
                line_height: 1.5,
                cell_width: 1.25,
                ..CellAdjust::default()
            },
            ..FontOptions::default()
        })
        .unwrap()
        .metrics();
        assert_eq!((metrics.width, metrics.height), (10, 30));

        let vertical = sprite('\u{2502}', metrics);
        assert_eq!((vertical.width, vertical.height), (10, 30));
        assert_eq!(vertical.top, metrics.baseline as i32);
        let column = (0..vertical.width)
            .find(|x| at(&vertical, *x, 0) == OPAQUE)
            .expect("the vertical line reaches the top edge");
        assert!(
            (0..vertical.height).all(|y| at(&vertical, column, y) == OPAQUE),
            "the vertical line has a gap inside the taller cell"
        );

        let horizontal = sprite('\u{2500}', metrics);
        let row = (0..horizontal.height)
            .find(|y| at(&horizontal, 0, *y) == OPAQUE)
            .expect("the horizontal line reaches the left edge");
        assert!(
            (0..horizontal.width).all(|x| at(&horizontal, x, row) == OPAQUE),
            "the horizontal line has a gap inside the wider cell"
        );

        let powerline = sprite('\u{e0b0}', metrics);
        assert_eq!((powerline.width, powerline.height), (10, 30));
    }

    #[test]
    fn a_corner_lines_up_with_the_straight_lines_it_joins() {
        let metrics = metrics(9, 21);
        let corner = sprite('\u{250c}', metrics);
        let horizontal = sprite('\u{2500}', metrics);
        let vertical = sprite('\u{2502}', metrics);
        let right_edge = |bitmap: &GlyphBitmap| -> Vec<u8> {
            (0..bitmap.height)
                .map(|y| at(bitmap, bitmap.width - 1, y))
                .collect()
        };
        let bottom_edge = |bitmap: &GlyphBitmap| -> Vec<u8> {
            (0..bitmap.width)
                .map(|x| at(bitmap, x, bitmap.height - 1))
                .collect()
        };
        assert_eq!(right_edge(&corner), right_edge(&horizontal));
        assert_eq!(bottom_edge(&corner), bottom_edge(&vertical));
    }

    #[test]
    fn a_rounded_corner_lines_up_with_the_straight_lines_it_joins() {
        let metrics = metrics(9, 21);
        let corner = sprite('\u{256d}', metrics);
        let horizontal = sprite('\u{2500}', metrics);
        let vertical = sprite('\u{2502}', metrics);
        let right: Vec<u8> = (0..21).map(|y| at(&corner, 8, y)).collect();
        let straight_right: Vec<u8> = (0..21).map(|y| at(&horizontal, 8, y)).collect();
        assert_eq!(right, straight_right);
        let bottom: Vec<u8> = (0..9).map(|x| at(&corner, x, 20)).collect();
        let straight_bottom: Vec<u8> = (0..9).map(|x| at(&vertical, x, 20)).collect();
        assert_eq!(bottom, straight_bottom);
    }

    #[test]
    fn a_double_corner_leaves_the_inside_open() {
        let metrics = metrics(9, 21);
        let corner = sprite('\u{2554}', metrics);
        let columns = pair(9, 1);
        let rows = pair(21, 1);
        let inside_x = (columns.first.1 + columns.second.0) / 2;
        let inside_y = (rows.first.1 + rows.second.0) / 2;
        assert_eq!(at(&corner, inside_x as u32, inside_y as u32), 0);
        assert_eq!(at(&corner, 8, rows.first.0 as u32), OPAQUE);
        assert_eq!(at(&corner, 8, rows.second.0 as u32), OPAQUE);
        assert_eq!(at(&corner, columns.first.0 as u32, 20), OPAQUE);
        assert_eq!(at(&corner, columns.second.0 as u32, 20), OPAQUE);
    }

    fn left_edge(bitmap: &GlyphBitmap) -> Vec<u8> {
        (0..bitmap.height).map(|y| at(bitmap, 0, y)).collect()
    }

    fn right_edge(bitmap: &GlyphBitmap) -> Vec<u8> {
        (0..bitmap.height)
            .map(|y| at(bitmap, bitmap.width - 1, y))
            .collect()
    }

    fn top_edge(bitmap: &GlyphBitmap) -> Vec<u8> {
        (0..bitmap.width).map(|x| at(bitmap, x, 0)).collect()
    }

    fn bottom_edge(bitmap: &GlyphBitmap) -> Vec<u8> {
        (0..bitmap.width)
            .map(|x| at(bitmap, x, bitmap.height - 1))
            .collect()
    }

    #[test]
    fn branch_lines_join_the_box_drawing_lines() {
        for metrics in [metrics(9, 21), metrics(8, 20), metrics(10, 30)] {
            let horizontal = sprite('\u{2500}', metrics);
            let vertical = sprite('\u{2502}', metrics);
            let empty = vec![0; metrics.width as usize];
            // Each symbol with the edges that its lines must reach.
            for (character, up, right, down, left) in [
                ('\u{f5d0}', false, true, false, true),
                ('\u{f5d1}', true, false, true, false),
                ('\u{f5d6}', false, true, true, false),
                ('\u{f5d9}', true, false, false, true),
                ('\u{f5da}', true, true, true, false),
                ('\u{f5e7}', true, true, true, true),
            ] {
                let bitmap = sprite(character, metrics);
                let expect = |reach: bool, line: Vec<u8>, blank: Vec<u8>| {
                    if reach {
                        line
                    } else {
                        blank
                    }
                };
                let blank_side = vec![0; metrics.height as usize];
                assert_eq!(
                    top_edge(&bitmap),
                    expect(up, top_edge(&vertical), empty.clone()),
                    "{:?} top",
                    character
                );
                assert_eq!(
                    bottom_edge(&bitmap),
                    expect(down, bottom_edge(&vertical), empty.clone()),
                    "{:?} bottom",
                    character
                );
                assert_eq!(
                    left_edge(&bitmap),
                    expect(left, left_edge(&horizontal), blank_side.clone()),
                    "{:?} left",
                    character
                );
                assert_eq!(
                    right_edge(&bitmap),
                    expect(right, right_edge(&horizontal), blank_side),
                    "{:?} right",
                    character
                );
            }
        }
    }

    #[test]
    fn commit_dot_stubs_join_the_box_drawing_lines() {
        let metrics = metrics(9, 21);
        let vertical = sprite('\u{2502}', metrics);
        let rows = centered(21, 1);
        let empty = vec![0; 9];
        // The dot is as wide as the cell, so it touches the side edges on the
        // line rows. A stub only matters on the edges that the dot does not
        // reach.
        for (character, up, down, sides) in [
            ('\u{f5ee}', false, false, false),
            ('\u{f5f4}', false, false, true),
            ('\u{f5fb}', true, true, false),
            ('\u{f5fc}', false, true, true),
            ('\u{f60d}', true, true, true),
        ] {
            let bitmap = sprite(character, metrics);
            let expect = |reach: bool, line: Vec<u8>| if reach { line } else { empty.clone() };
            assert_eq!(
                top_edge(&bitmap),
                expect(up, top_edge(&vertical)),
                "{:?}",
                character
            );
            assert_eq!(
                bottom_edge(&bitmap),
                expect(down, bottom_edge(&vertical)),
                "{:?}",
                character
            );
            if sides {
                for y in rows.0..rows.1 {
                    assert_eq!(at(&bitmap, 8, y as u32), OPAQUE, "{:?}", character);
                }
            }
        }
    }

    #[test]
    fn a_filled_commit_dot_covers_its_centre_and_an_empty_one_does_not() {
        let metrics = metrics(9, 21);
        let rows = centered(21, 1);
        let columns = centered(9, 1);
        let (x, y) = (columns.0 as u32, rows.0 as u32);
        for (filled, empty) in [('\u{f5ee}', '\u{f5ef}'), ('\u{f5fa}', '\u{f5fb}')] {
            assert_eq!(at(&sprite(filled, metrics), x, y), OPAQUE, "{:?}", filled);
            assert_eq!(at(&sprite(empty, metrics), x, y), 0, "{:?}", empty);
        }
    }

    #[test]
    fn a_fading_line_fades_toward_its_named_edge() {
        let metrics = metrics(9, 21);
        let rows = centered(21, 1);
        let columns = centered(9, 1);
        let (mid_x, mid_y) = (columns.0 as u32, rows.0 as u32);
        let right = sprite('\u{f5d2}', metrics);
        assert!(at(&right, 0, mid_y) > at(&right, 8, mid_y));
        let left = sprite('\u{f5d3}', metrics);
        assert!(at(&left, 0, mid_y) < at(&left, 8, mid_y));
        let down = sprite('\u{f5d4}', metrics);
        assert!(at(&down, mid_x, 0) > at(&down, mid_x, 20));
        let up = sprite('\u{f5d5}', metrics);
        assert!(at(&up, mid_x, 0) < at(&up, mid_x, 20));
    }

    #[test]
    fn a_wide_budget_spans_both_cells() {
        let bitmap = draw('\u{2588}', metrics(8, 20), 2).unwrap();
        assert_eq!(bitmap.width, 16);
    }

    #[test]
    fn other_characters_are_left_to_the_font() {
        assert!(draw('a', metrics(8, 20), 1).is_none());
        assert!(draw('\u{e0b4}', metrics(8, 20), 1).is_none());
        assert!(!is_sprite('\u{25a0}'));
        assert!(!is_sprite('\u{f5cf}'));
        assert!(!is_sprite('\u{f60e}'));
    }
}
