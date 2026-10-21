use ab_glyph::FontVec;
use criterion::{Criterion, black_box, criterion_group, criterion_main};
use light_fw_core as light;

use light::layout::{Layout, Point, Rect, TextMetrics};
use light::render::{Color, RendererBackend};
use light::vdom::{NodeKind, VElement, diff};

const ROWS: usize = 100;
const CELLS: usize = 9;
const CHANGED: usize = 10;

struct StubMetrics;

impl TextMetrics for StubMetrics {
    fn measure(&self, text: &str, _font_size: f32) -> (f32, f32) {
        (text.chars().count() as f32 * 8.0, 16.0)
    }
}

struct MemoryBackend {
    width: u32,
    height: u32,
    frame: Vec<u8>,
}

impl MemoryBackend {
    fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            frame: vec![0; (width * height * 4) as usize],
        }
    }
}

impl RendererBackend for MemoryBackend {
    fn resize(&mut self, width: u32, height: u32) -> light::Result<()> {
        self.width = width;
        self.height = height;
        self.frame = vec![0; (width * height * 4) as usize];
        Ok(())
    }

    fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn frame_mut(&mut self) -> &mut [u8] {
        &mut self.frame
    }

    fn present(&mut self) -> light::Result<()> {
        Ok(())
    }
}

fn build_tree() -> VElement {
    let mut rows = Vec::with_capacity(ROWS);
    for row in 0..ROWS {
        let row_key: &'static str = Box::leak(format!("row-{row}").into_boxed_str());
        let mut cells = Vec::with_capacity(CELLS);
        for cell in 0..CELLS {
            let cell_key: &'static str = Box::leak(format!("cell-{row}-{cell}").into_boxed_str());
            cells.push(VElement::leaf(
                cell_key,
                NodeKind::Text {
                    content: format!("cell {row}-{cell}"),
                },
            ));
        }
        rows.push(VElement::container(row_key, NodeKind::Row, cells));
    }
    VElement::container("root", NodeKind::Column, rows)
}

fn changed_tree(tree: &VElement) -> VElement {
    let mut changed = tree.clone();
    for row in changed.children.iter_mut().take(CHANGED) {
        if let Some(cell) = row.children.first_mut() {
            cell.kind = NodeKind::Text {
                content: format!("changed {}", row.key),
            };
        }
    }
    changed
}

fn bench_diff(c: &mut Criterion) {
    let tree = build_tree();
    let changed = changed_tree(&tree);
    c.bench_function("vdom/diff_1000_elements_10_changes", |b| {
        b.iter(|| black_box(diff(black_box(&tree), black_box(&changed))));
    });
}

fn bench_layout(c: &mut Criterion) {
    let tree = build_tree();
    let metrics = StubMetrics;
    c.bench_function("layout/flex_1000_elements", |b| {
        b.iter(|| black_box(Layout::build(black_box(&tree), (1280.0, 1600.0), &metrics)));
    });
}

fn load_font() -> Option<FontVec> {
    const FONT_PATHS: &[&str] = &[
        r"C:\Windows\Fonts\segoeui.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        "/System/Library/Fonts/Supplemental/Arial.ttf",
    ];
    for path in FONT_PATHS {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        if let Ok(font) = FontVec::try_from_vec(bytes) {
            return Some(font);
        }
    }
    None
}

fn bench_render(c: &mut Criterion) {
    let mut group = c.benchmark_group("render");
    let mut backend = MemoryBackend::new(800, 600);

    group.bench_function("rects_1000", |b| {
        b.iter(|| {
            for index in 0..1000 {
                let x = (index % 40) as f32 * 20.0;
                let y = (index / 40) as f32 * 20.0;
                backend.draw_rect(Rect::new(x, y, 18.0, 18.0), black_box(0x33_66_CC_FF));
            }
            black_box(backend.frame_mut());
        });
    });

    if let Some(font) = load_font() {
        group.bench_function("text_100", |b| {
            b.iter(|| {
                for index in 0..100 {
                    let x = (index / 20) as f32 * 260.0;
                    let y = (index % 20) as f32 * 24.0;
                    backend.draw_text(
                        &font,
                        "The quick brown fox jumps over the lazy dog",
                        Point::new(x, y),
                        black_box(0x1F_24_2B_FF as Color),
                        16.0,
                    );
                }
                black_box(backend.frame_mut());
            });
        });
    } else {
        eprintln!("render/text_100 skipped: no system font found");
    }

    group.finish();
}

criterion_group!(benches, bench_diff, bench_layout, bench_render);
criterion_main!(benches);
