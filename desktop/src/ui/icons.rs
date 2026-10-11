//! Authored vector geometry. No SF Symbol font data and no Unicode icon stand-ins.
//! Coordinates use a 24-point optical grid, scaled to each control's requested icon box.
use eframe::egui::{self, Color32, Painter, Pos2, Rect, Shape, Stroke};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    Sparkles,
    Layers,
    Plus,
    PointAsk,
    Close,
    Back,
    ChevronDown,
    ChevronUpDown,
    Reply,
    Open,
    Dismiss,
    Mic,
    MicFill,
    Waveform,
    Send,
    ArrowUp,
    Return,
    Cpu,
    Folder,
    Terminal,
    Window,
    Chat,
    Copy,
    Check,
    Hourglass,
    Warning,
    Reselect,
    Link,
    TextCursor,
    Wifi,
    Video,
    Photo,
    Settings,
    Leaf,
    Info,
    Account,
    Mail,
    Play,
    Exclamation,
}

struct Pen<'a> {
    painter: &'a Painter,
    rect: Rect,
    color: Color32,
    stroke: Stroke,
}

impl Pen<'_> {
    fn p(&self, x: f32, y: f32) -> Pos2 {
        egui::pos2(self.rect.left() + x * self.rect.width() / 24.0, self.rect.top() + y * self.rect.height() / 24.0)
    }
    fn line(&self, points: &[(f32, f32)]) {
        self.painter.add(Shape::line(points.iter().map(|&(x, y)| self.p(x, y)).collect(), self.stroke));
    }
    fn closed(&self, points: &[(f32, f32)], fill: bool) {
        let points = points.iter().map(|&(x, y)| self.p(x, y)).collect();
        self.painter.add(if fill {
            Shape::convex_polygon(points, self.color, Stroke::NONE)
        } else {
            Shape::closed_line(points, self.stroke)
        });
    }
    fn circle(&self, x: f32, y: f32, r: f32, fill: bool) {
        let r = r * self.rect.width() / 24.0;
        self.painter.circle(
            self.p(x, y),
            r,
            if fill { self.color } else { Color32::TRANSPARENT },
            if fill { Stroke::NONE } else { self.stroke },
        );
    }
    fn rect(&self, a: (f32, f32), b: (f32, f32), radius: f32, fill: bool) {
        self.painter.rect(
            Rect::from_min_max(self.p(a.0, a.1), self.p(b.0, b.1)),
            radius * self.rect.width() / 24.0,
            if fill { self.color } else { Color32::TRANSPARENT },
            if fill { Stroke::NONE } else { self.stroke },
            egui::StrokeKind::Inside,
        );
    }
    fn arc(&self, center: (f32, f32), radius: f32, start: f32, end: f32) {
        let points = (0..=20)
            .map(|i| {
                let a = start + (end - start) * i as f32 / 20.0;
                self.p(center.0 + radius * a.cos(), center.1 + radius * a.sin())
            })
            .collect();
        self.painter.add(Shape::line(points, self.stroke));
    }
    fn sparkle(&self, x: f32, y: f32, r: f32) {
        // Four convex triangles form the concave sparkle without relying on convex tessellation.
        let c = self.p(x, y);
        let pts = [
            (x, y - r),
            (x + r * 0.25, y - r * 0.25),
            (x + r, y),
            (x + r * 0.25, y + r * 0.25),
            (x, y + r),
            (x - r * 0.25, y + r * 0.25),
            (x - r, y),
            (x - r * 0.25, y - r * 0.25),
        ];
        for i in 0..8 {
            self.painter.add(Shape::convex_polygon(
                vec![c, self.p(pts[i].0, pts[i].1), self.p(pts[(i + 1) % 8].0, pts[(i + 1) % 8].1)],
                self.color,
                Stroke::NONE,
            ));
        }
    }
}

pub fn paint_icon(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let p = Pen { painter, rect, color, stroke: Stroke::new((rect.width() / 24.0 * 1.8).max(0.9), color) };
    use Icon::*;
    match icon {
        Sparkles => {
            p.sparkle(10.0, 14.0, 8.0);
            p.sparkle(18.5, 5.5, 4.5);
            p.sparkle(20.0, 19.0, 2.5);
        }
        Plus => {
            p.line(&[(12.0, 4.0), (12.0, 20.0)]);
            p.line(&[(4.0, 12.0), (20.0, 12.0)]);
        }
        Close => {
            p.line(&[(5.0, 5.0), (19.0, 19.0)]);
            p.line(&[(19.0, 5.0), (5.0, 19.0)]);
        }
        Back => p.line(&[(15.0, 4.0), (7.0, 12.0), (15.0, 20.0)]),
        ChevronDown => p.line(&[(6.0, 9.0), (12.0, 15.0), (18.0, 9.0)]),
        ChevronUpDown => {
            p.line(&[(8.0, 8.0), (12.0, 4.0), (16.0, 8.0)]);
            p.line(&[(8.0, 16.0), (12.0, 20.0), (16.0, 16.0)]);
        }
        Layers => {
            p.closed(&[(12.0, 2.5), (22.0, 8.0), (12.0, 13.5), (2.0, 8.0)], false);
            p.line(&[(2.0, 12.5), (12.0, 18.0), (22.0, 12.5)]);
            p.line(&[(2.0, 17.0), (12.0, 22.5), (22.0, 17.0)]);
        }
        Dismiss | Check => p.line(&[(4.0, 12.0), (9.0, 17.0), (20.0, 6.0)]),
        ArrowUp => {
            p.line(&[(12.0, 20.0), (12.0, 4.0)]);
            p.line(&[(5.0, 11.0), (12.0, 4.0), (19.0, 11.0)]);
        }
        Send => {
            p.closed(&[(3.0, 3.0), (22.0, 12.0), (3.0, 21.0), (7.0, 12.0)], false);
            p.line(&[(7.0, 12.0), (22.0, 12.0)]);
        }
        Reply => {
            p.line(&[(10.0, 5.0), (3.0, 11.0), (10.0, 17.0), (10.0, 13.0)]);
            p.line(&[(3.0, 11.0), (13.0, 11.0), (18.0, 13.0), (21.0, 20.0), (21.0, 13.0), (18.0, 8.0), (10.0, 8.0), (10.0, 5.0)]);
        }
        Open => {
            p.line(&[(12.0, 4.0), (4.0, 4.0), (4.0, 20.0), (20.0, 20.0), (20.0, 12.0)]);
            p.line(&[(12.0, 12.0), (21.0, 3.0), (14.0, 3.0)]);
            p.line(&[(21.0, 3.0), (21.0, 10.0)]);
        }
        Mic | MicFill => {
            p.rect((9.0, 2.0), (15.0, 15.0), 3.0, icon == MicFill);
            p.arc((12.0, 12.0), 7.0, 0.0, std::f32::consts::PI);
            p.line(&[(12.0, 19.0), (12.0, 23.0)]);
            p.line(&[(8.0, 23.0), (16.0, 23.0)]);
        }
        Waveform => {
            for (x, a, b) in [(3.0, 9.0, 15.0), (7.5, 5.0, 19.0), (12.0, 1.0, 23.0), (16.5, 7.0, 17.0), (21.0, 4.0, 20.0)] {
                p.line(&[(x, a), (x, b)]);
            }
        }
        Return => {
            p.line(&[(20.0, 5.0), (20.0, 14.0), (4.0, 14.0)]);
            p.line(&[(10.0, 8.0), (4.0, 14.0), (10.0, 20.0)]);
        }
        Folder => p.closed(&[(2.0, 6.0), (10.0, 6.0), (12.0, 8.0), (22.0, 8.0), (22.0, 20.0), (2.0, 20.0)], false),
        Terminal => {
            p.rect((2.0, 4.0), (22.0, 20.0), 2.0, false);
            p.line(&[(6.0, 9.0), (10.0, 12.0), (6.0, 15.0)]);
            p.line(&[(12.0, 15.0), (18.0, 15.0)]);
        }
        Window => {
            p.rect((2.0, 4.0), (22.0, 20.0), 2.0, false);
            p.circle(5.0, 7.0, 0.6, true);
            p.circle(8.0, 7.0, 0.6, true);
            p.circle(11.0, 7.0, 0.6, true);
        }
        Chat => {
            p.rect((2.0, 3.0), (19.0, 17.0), 3.0, false);
            p.line(&[(5.0, 17.0), (5.0, 21.0), (10.0, 17.0)]);
            p.line(&[(19.0, 8.0), (23.0, 8.0), (23.0, 21.0), (19.0, 19.0), (13.0, 19.0)]);
        }
        Copy => {
            p.rect((8.0, 8.0), (21.0, 22.0), 2.0, false);
            p.line(&[(16.0, 8.0), (16.0, 2.0), (2.0, 2.0), (2.0, 16.0), (8.0, 16.0)]);
        }
        Cpu => {
            p.rect((6.0, 6.0), (18.0, 18.0), 2.0, false);
            p.rect((9.0, 9.0), (15.0, 15.0), 1.0, false);
            for v in [8.0, 12.0, 16.0] {
                p.line(&[(v, 2.0), (v, 6.0)]);
                p.line(&[(v, 18.0), (v, 22.0)]);
                p.line(&[(2.0, v), (6.0, v)]);
                p.line(&[(18.0, v), (22.0, v)]);
            }
        }
        Hourglass => {
            p.line(&[(5.0, 2.0), (19.0, 2.0)]);
            p.line(&[(5.0, 22.0), (19.0, 22.0)]);
            p.closed(
                &[
                    (7.0, 2.0),
                    (17.0, 2.0),
                    (17.0, 7.0),
                    (12.0, 12.0),
                    (17.0, 17.0),
                    (17.0, 22.0),
                    (7.0, 22.0),
                    (7.0, 17.0),
                    (12.0, 12.0),
                    (7.0, 7.0),
                ],
                false,
            );
        }
        Warning => {
            p.closed(&[(12.0, 2.0), (23.0, 21.0), (1.0, 21.0)], false);
            p.line(&[(12.0, 8.0), (12.0, 14.0)]);
            p.circle(12.0, 18.0, 0.9, true);
        }
        Exclamation => {
            p.line(&[(12.0, 3.0), (12.0, 14.0)]);
            p.circle(12.0, 20.0, 1.2, true);
        }
        Reselect => {
            p.line(&[(3.0, 9.0), (3.0, 3.0), (9.0, 3.0)]);
            p.line(&[(15.0, 3.0), (21.0, 3.0), (21.0, 9.0)]);
            p.line(&[(21.0, 15.0), (21.0, 21.0), (15.0, 21.0)]);
            p.line(&[(9.0, 21.0), (3.0, 21.0), (3.0, 15.0)]);
        }
        PointAsk => {
            // Two overlapping selection rectangles, the rear one dashed, plus a cursor.
            for points in
                [[(2.0, 7.0), (2.0, 2.0), (7.0, 2.0)], [(10.0, 2.0), (14.0, 2.0), (14.0, 4.0)], [(2.0, 10.0), (2.0, 14.0), (4.0, 14.0)]]
            {
                p.line(&points);
            }
            p.line(&[(12.0, 20.0), (6.0, 20.0), (6.0, 6.0), (20.0, 6.0), (20.0, 12.0)]);
            p.closed(&[(14.0, 12.0), (14.0, 23.0), (17.0, 20.0), (20.0, 24.0), (22.0, 22.5), (19.0, 18.5), (23.0, 18.5)], false);
        }
        Link => {
            p.arc((8.0, 16.0), 5.0, 0.6, 4.4);
            p.arc((16.0, 8.0), 5.0, 3.75, 7.55);
            p.line(&[(7.0, 17.0), (17.0, 7.0)]);
        }
        TextCursor => {
            p.line(&[(12.0, 3.0), (12.0, 21.0)]);
            p.line(&[(8.0, 3.0), (16.0, 3.0)]);
            p.line(&[(8.0, 21.0), (16.0, 21.0)]);
        }
        Wifi => {
            p.arc((12.0, 22.0), 19.0, 4.05, 5.37);
            p.arc((12.0, 22.0), 13.0, 4.05, 5.37);
            p.arc((12.0, 22.0), 7.0, 4.05, 5.37);
            p.circle(12.0, 21.0, 1.4, true);
        }
        Video => {
            p.rect((2.0, 5.0), (16.0, 19.0), 2.5, false);
            p.closed(&[(16.0, 9.0), (23.0, 5.0), (23.0, 19.0), (16.0, 15.0)], false);
        }
        Photo => {
            p.rect((2.0, 3.0), (22.0, 21.0), 2.0, false);
            p.circle(8.0, 8.0, 2.0, false);
            p.line(&[(3.0, 18.0), (10.0, 11.0), (15.0, 16.0), (18.0, 13.0), (22.0, 17.0)]);
        }
        Settings => {
            p.circle(12.0, 12.0, 4.0, false);
            let pts = (0..24)
                .map(|i| {
                    let a = i as f32 * std::f32::consts::TAU / 24.0;
                    let r = if i % 3 == 0 { 11.0 } else { 8.5 };
                    (12.0 + a.cos() * r, 12.0 + a.sin() * r)
                })
                .collect::<Vec<_>>();
            p.closed(&pts, false);
        }
        Leaf => {
            p.line(&[(4.0, 21.0), (18.0, 7.0)]);
            p.closed(&[(5.0, 18.0), (3.0, 12.0), (6.0, 6.0), (14.0, 4.0), (22.0, 2.0), (20.0, 12.0), (16.0, 18.0), (10.0, 20.0)], false);
        }
        Info => {
            p.circle(12.0, 12.0, 10.0, false);
            p.circle(12.0, 6.5, 1.0, true);
            p.line(&[(10.0, 11.0), (12.0, 11.0), (12.0, 18.0)]);
            p.line(&[(9.0, 18.0), (15.0, 18.0)]);
        }
        Account => {
            p.circle(12.0, 7.0, 4.0, false);
            p.arc((12.0, 22.0), 8.0, std::f32::consts::PI, std::f32::consts::TAU);
        }
        Mail => {
            p.rect((2.0, 4.0), (22.0, 20.0), 2.0, false);
            p.line(&[(3.0, 6.0), (12.0, 13.0), (21.0, 6.0)]);
        }
        Play => p.closed(&[(7.0, 3.0), (22.0, 12.0), (7.0, 21.0)], true),
    }
}
