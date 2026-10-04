//! mermaid flowchart 子集:解析 + Sugiyama-lite 布局 → 纯几何绘制指令。
//!
//! 输入是 ```` ```mermaid ```` 围栏里的纯文本(由调用方按纯文本取出,
//! 不经第二个 Markdown 解析器——铁律 1 在此的形态是「围栏内容当 DSL 文本,
//! 不当 Markdown」);输出只含坐标/尺寸/线型/颜色语义/层级的绘制指令
//! (铁律 2:不依赖 egui 或任何 UI 框架,app 与将来的 PDF 导出都能翻译)。
//!
//! v1 只支持 flowchart/graph TD|LR 的节点(`[]` 圆角矩形/`()` 跑道形/
//! `{}` 菱形)、四种连线(`---` `-->` `-.->` `==>`,可带 `|文字|` 标签)、
//! `%%` 注释行与空行;其余语法(其他图类型、subgraph/classDef/style/
//! linkStyle 等)显式返回 `MermaidError`,由上层整体回落为源码高亮
//! 代码块——不猜测、不吞错。
//!
//! 坐标系:原点在图左上角,TD 方向 x 向右、y 向下且 y 随流向增大;
//! LR 方向 x 随流向增大。颜色不落色值,只给 `ColorSlot` 语义槽,
//! 由后端翻译为当帧主题色(明/暗自适应)。

mod layout;
mod parser;

pub use layout::{layout, CHAR_W, LAYER_GAP, LINE_H, NODE_GAP, PAD_X, PAD_Y};
pub use parser::{parse, EdgeDecl, Flowchart, NodeDecl};

use std::fmt;

/// 解析并布局 mermaid 源文本(默认按字符计数估算文字宽度,CJK 记 2 倍)。
///
/// 围栏内容原样传入;语法超出 v1 子集、图为空或检测到环时返回
/// [`MermaidError`],调用方据此回落为源码代码块。
pub fn render(source: &str) -> Result<Diagram, MermaidError> {
    render_with(source, &CharCountMeasurer)
}

/// 同 [`render`],但允许注入文字测量实现(如 app 层拿 egui Fonts 做
/// 真实度量回调)。布局算法不感知测量来源,只消费 [`TextMeasurer`]。
pub fn render_with(source: &str, measure: &dyn TextMeasurer) -> Result<Diagram, MermaidError> {
    layout(&parse(source)?, measure)
}

/// 文字测量接口:布局用它把「估算宽度」与「真实字体度量」解耦。
///
/// v1 默认 [`CharCountMeasurer`](半角 [`CHAR_W`],CJK 记 2 倍宽,行高
/// [`LINE_H`]);取舍:估算零依赖、无头可跑,但窄字/宽字/emoji 会有
/// 逐字符误差——app 层渲染前可用真实度量回调替换重排,接口已留出。
pub trait TextMeasurer {
    /// 单行文本的尺寸(宽 × 高)。
    fn measure(&self, text: &str) -> Size;
}

/// 默认测量:按字符计数估宽(CJK 等全角记 2 倍),行高取 [`LINE_H`]。
#[derive(Debug, Clone, Copy, Default)]
pub struct CharCountMeasurer;

impl TextMeasurer for CharCountMeasurer {
    fn measure(&self, text: &str) -> Size {
        let units: f32 = text
            .chars()
            .map(|c| if is_cjk_wide(c) { 2.0 } else { 1.0 })
            .sum();
        Size {
            w: units * CHAR_W,
            h: LINE_H,
        }
    }
}

/// CJK 等全角字符的宽度判定(覆盖谚文/注音/假名/CJK 表意与兼容/
/// 全角形式等常用区段;emoji 按半角估,误差由可注入测量兜底)。
fn is_cjk_wide(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x115F      // 谚文字母
        | 0x2E80..=0x303E    // CJK 部首与符号
        | 0x3041..=0x33FF    // 假名/注音/谚文音节/CJK 兼容
        | 0x3400..=0x4DBF    // 表意扩展 A
        | 0x4E00..=0x9FFF    // 统一表意
        | 0xA960..=0xA97F    // 谚文扩展 A/B
        | 0xAC00..=0xD7FF    // 谚文音节与兼容
        | 0xF900..=0xFAFF    // 兼容表意
        | 0xFE30..=0xFE4F    // CJK 兼容形式
        | 0xFF00..=0xFF60    // 全角形式
        | 0xFFE0..=0xFFE6
        | 0x20000..=0x3FFFD  // 表意扩展 B 及以后
    )
}

// ---- 纯几何与语义原语(不含任何 UI 框架类型) ----

/// 平面坐标(px)。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    /// 横坐标。
    pub x: f32,
    /// 纵坐标。
    pub y: f32,
}

/// 尺寸(px)。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Size {
    /// 宽。
    pub w: f32,
    /// 高。
    pub h: f32,
}

/// 轴对齐矩形:左上角 + 宽高。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    /// 左上角。
    pub min: Point,
    /// 宽高。
    pub size: Size,
}

impl Rect {
    /// 几何中心。
    pub fn center(&self) -> Point {
        Point {
            x: self.min.x + self.size.w / 2.0,
            y: self.min.y + self.size.h / 2.0,
        }
    }

    /// 右边缘 x。
    pub fn max_x(&self) -> f32 {
        self.min.x + self.size.w
    }

    /// 下边缘 y。
    pub fn max_y(&self) -> f32 {
        self.min.y + self.size.h
    }
}

/// 图方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// TD:自上而下。
    TopDown,
    /// LR:左到右。
    LeftRight,
}

/// 节点形状(v1 三种;`[]` 与 `()` 都是圆角矩形,后者圆角更大)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// `A[标签]`:小圆角矩形。
    RoundedRect,
    /// `A(标签)`:跑道形(圆角半径取盒高一半)。
    Stadium,
    /// `A{标签}`:菱形,四个顶点为包围盒各边中点。
    Diamond,
}

/// 连线线型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineStyle {
    /// `-->` / `---` 实线。
    Solid,
    /// `-.->` 虚线。
    Dashed,
    /// `==>` 粗线。
    Thick,
}

/// 颜色语义槽:IR 不携带具体色值,由后端翻译为当帧主题色(明/暗自适应,
/// 不硬编码 RGB)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorSlot {
    /// 节点盒填充。
    NodeFill,
    /// 节点盒描边。
    NodeStroke,
    /// 连线与箭头。
    Line,
    /// 文字(节点标签与边标签)。
    Text,
}

/// 绘制层级:值小者先画(边压在盒下,文字最后)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ZLayer {
    /// 连线。
    Edge,
    /// 节点盒。
    Node,
    /// 文字。
    Label,
}

/// 一个节点的绘制指令:形状包围盒 + 文字锚点。
#[derive(Debug, Clone, PartialEq)]
pub struct NodeDraw {
    /// 节点 ID(与源文本一致,可作稳定 widget id 的素材)。
    pub id: String,
    /// 居中显示的标签文字。
    pub label: String,
    /// 形状。
    pub shape: Shape,
    /// 形状包围盒。
    pub rect: Rect,
    /// 文字锚点(包围盒中心,后端以此为中心排一行文字)。
    pub text_anchor: Point,
    /// 填充色槽。
    pub fill: ColorSlot,
    /// 描边色槽。
    pub stroke: ColorSlot,
    /// 绘制层级。
    pub z: ZLayer,
}

/// 一条边的绘制指令:折线路径 + 箭头标记 + 可选标签锚点。
#[derive(Debug, Clone, PartialEq)]
pub struct EdgePath {
    /// 起点 ID。
    pub from: String,
    /// 终点 ID。
    pub to: String,
    /// 线型。
    pub style: LineStyle,
    /// 终点端是否画箭头(`---` 无向为 false)。
    pub arrow: bool,
    /// 折线顶点(含起终点,顺序即走线方向;箭头朝向由最后两顶点决定)。
    pub points: Vec<Point>,
    /// 边标签文字(`|文字|`)。
    pub label: Option<String>,
    /// 标签锚点(路线中点;后端按 [`ZLayer::Label`] 层画文字)。
    pub label_anchor: Option<Point>,
    /// 描边色槽。
    pub stroke: ColorSlot,
    /// 折线与箭头的绘制层级。
    pub z: ZLayer,
}

/// 布局完成的图:全部绘制指令 + 整图尺寸。
#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    /// 布局方向;坐标已按方向就位(TD:y 随流向增大;LR:x 随流向增大)。
    pub direction: Direction,
    /// 整图尺寸(原点在 (0,0))。
    pub bounds: Size,
    /// 节点指令(声明序)。
    pub nodes: Vec<NodeDraw>,
    /// 边指令(声明序)。
    pub edges: Vec<EdgePath>,
}

/// mermaid 解析/布局错误:面向用户的可读信息,上层据此回落为源码代码块。
#[derive(Debug, Clone, PartialEq)]
pub struct MermaidError {
    /// 1 起始行号;0 表示不锚定具体行(如环检测)。
    pub line: usize,
    /// 人可读的错误描述(中文)。
    pub message: String,
}

impl fmt::Display for MermaidError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            write!(f, "mermaid:{}", self.message)
        } else {
            write!(f, "mermaid 第 {} 行:{}", self.line, self.message)
        }
    }
}

impl std::error::Error for MermaidError {}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
flowchart TD
  A[开始] --> B{判断}
  B -->|是| C(处理)
  B -->|否| D[结束]
  C --> D
";

    fn node_rect(d: &Diagram, id: &str) -> Rect {
        d.nodes
            .iter()
            .find(|n| n.id == id)
            .expect("节点应存在")
            .rect
    }

    fn center(d: &Diagram, id: &str) -> Point {
        node_rect(d, id).center()
    }

    fn rects_overlap(a: Rect, b: Rect) -> bool {
        const EPS: f32 = 0.01;
        a.min.x < b.max_x() - EPS
            && b.min.x < a.max_x() - EPS
            && a.min.y < b.max_y() - EPS
            && b.min.y < a.max_y() - EPS
    }

    fn segments_intersect(p1: Point, p2: Point, p3: Point, p4: Point) -> bool {
        let orient =
            |a: Point, b: Point, c: Point| (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
        let d1 = orient(p3, p4, p1);
        let d2 = orient(p3, p4, p2);
        let d3 = orient(p1, p2, p3);
        let d4 = orient(p1, p2, p4);
        ((d1 > 0.0) != (d2 > 0.0)) && ((d3 > 0.0) != (d4 > 0.0))
    }

    /// 宽松断言的穿盒判定:盒四边内缩 1.5px(端点落在盒边框上不算穿越),
    /// 线段任一端点在内缩盒内、或与四边严格相交即算穿越。
    fn segment_hits_box(seg: (Point, Point), rect: Rect) -> bool {
        const M: f32 = 1.5;
        let w = (rect.size.w - 2.0 * M).max(0.0);
        let h = (rect.size.h - 2.0 * M).max(0.0);
        if w <= 0.0 || h <= 0.0 {
            return false;
        }
        let shrunk = Rect {
            min: Point {
                x: rect.min.x + M,
                y: rect.min.y + M,
            },
            size: Size { w, h },
        };
        let inside = |p: Point| {
            p.x > shrunk.min.x && p.x < shrunk.max_x() && p.y > shrunk.min.y && p.y < shrunk.max_y()
        };
        if inside(seg.0) || inside(seg.1) {
            return true;
        }
        let c1 = shrunk.min;
        let c2 = Point {
            x: shrunk.max_x(),
            y: shrunk.min.y,
        };
        let c3 = Point {
            x: shrunk.max_x(),
            y: shrunk.max_y(),
        };
        let c4 = Point {
            x: shrunk.min.x,
            y: shrunk.max_y(),
        };
        [(c1, c2), (c2, c3), (c3, c4), (c4, c1)]
            .iter()
            .any(|&(a, b)| segments_intersect(seg.0, seg.1, a, b))
    }

    #[test]
    fn sample_counts_and_semantic_slots() {
        let d = render(SAMPLE).unwrap();
        assert_eq!(d.nodes.len(), 4, "盒数应等于声明数:{:?}", d.nodes);
        assert_eq!(d.edges.len(), 4);
        assert_eq!(d.direction, Direction::TopDown);
        let find = |id: &str| d.nodes.iter().find(|n| n.id == id).unwrap();
        assert_eq!(
            (find("A").shape, find("A").label.as_str()),
            (Shape::RoundedRect, "开始")
        );
        assert_eq!(
            (find("B").shape, find("B").label.as_str()),
            (Shape::Diamond, "判断")
        );
        assert_eq!(
            (find("C").shape, find("C").label.as_str()),
            (Shape::Stadium, "处理")
        );
        assert_eq!(
            (find("D").shape, find("D").label.as_str()),
            (Shape::RoundedRect, "结束")
        );
        assert!(d.nodes.iter().all(|n| n.z == ZLayer::Node
            && n.fill == ColorSlot::NodeFill
            && n.stroke == ColorSlot::NodeStroke));
        assert!(d
            .edges
            .iter()
            .all(|e| e.z == ZLayer::Edge && e.stroke == ColorSlot::Line));
        assert!(d.nodes.iter().all(|n| n.text_anchor == n.rect.center()));
    }

    #[test]
    fn td_edges_flow_downward() {
        let d = render(SAMPLE).unwrap();
        for e in &d.edges {
            assert!(
                center(&d, &e.to).y > center(&d, &e.from).y,
                "{}→{} 应自上而下",
                e.from,
                e.to
            );
        }
    }

    #[test]
    fn no_two_boxes_overlap() {
        let d = render(SAMPLE).unwrap();
        for (i, a) in d.nodes.iter().enumerate() {
            for b in d.nodes.iter().skip(i + 1) {
                assert!(
                    !rects_overlap(a.rect, b.rect),
                    "{} 与 {} 的盒重叠:{:?} {:?}",
                    a.id,
                    b.id,
                    a.rect,
                    b.rect
                );
            }
        }
    }

    #[test]
    fn edge_polylines_avoid_boxes() {
        // 任务书要求「宽松断言」;虚拟节点把跨层长边也拆成相邻层链后,
        // 正交肘形对全部边都不穿盒,断言按严格口径跑(含端点盒自身——
        // 内缩 1.5px 后贴边不算穿越)。两个样本:纯相邻层 + 含跨层长边。
        for src in [
            SAMPLE,
            "flowchart TD\nA --> B\nA --> C\nB --> C\nC ==>|长边| D",
        ] {
            let d = render(src).unwrap();
            for e in &d.edges {
                for node in &d.nodes {
                    for seg in e.points.windows(2) {
                        let seg = (seg[0], seg[1]);
                        assert!(
                            !segment_hits_box(seg, node.rect),
                            "{}→{} 的线段 {:?} 穿过 {} 的盒",
                            e.from,
                            e.to,
                            seg,
                            node.id
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn lr_direction_is_td_transposed() {
        let td = render(SAMPLE).unwrap();
        let lr = render(&SAMPLE.replacen("TD", "LR", 1)).unwrap();
        assert_eq!(lr.direction, Direction::LeftRight);
        assert_eq!((lr.bounds.w, lr.bounds.h), (td.bounds.h, td.bounds.w));
        let sw = |p: Point| Point { x: p.y, y: p.x };
        for (t, l) in td.nodes.iter().zip(&lr.nodes) {
            assert_eq!(l.rect.min, sw(t.rect.min));
            assert_eq!(
                (l.rect.size.w, l.rect.size.h),
                (t.rect.size.h, t.rect.size.w)
            );
            assert_eq!(l.text_anchor, sw(t.text_anchor));
        }
        for (t, l) in td.edges.iter().zip(&lr.edges) {
            assert_eq!(
                l.points,
                t.points.iter().map(|p| sw(*p)).collect::<Vec<_>>()
            );
        }
        for e in &lr.edges {
            assert!(
                center(&lr, &e.to).x > center(&lr, &e.from).x,
                "{}→{} 应从左到右",
                e.from,
                e.to
            );
        }
    }

    #[test]
    fn routes_stay_between_source_and_target() {
        // 含跨层长边(A→C 跨两层):折线各段 y 单调不减,且全部落在
        // 源盒下边缘与目标盒上边缘之间。
        let d = render("flowchart TD\nA --> B\nA --> C\nB --> C").unwrap();
        for e in &d.edges {
            let src_bottom = node_rect(&d, &e.from).max_y();
            let dst_top = node_rect(&d, &e.to).min.y;
            let ys: Vec<f32> = e.points.iter().map(|p| p.y).collect();
            assert!(
                ys.windows(2).all(|w| w[0] <= w[1] + 0.01),
                "折线应单调向下:{:?}",
                e.points
            );
            for &y in &ys {
                assert!(
                    y >= src_bottom - 0.01 && y <= dst_top + 0.01,
                    "折线越出 [{},{}] 区间:{:?}",
                    src_bottom,
                    dst_top,
                    e.points
                );
            }
        }
    }

    #[test]
    fn labeled_edge_carries_anchor_on_route() {
        let d = render(SAMPLE).unwrap();
        let e = d
            .edges
            .iter()
            .find(|e| e.label.as_deref() == Some("是"))
            .expect("应有一条「是」边");
        let anchor = e.label_anchor.expect("带标签的边应有锚点");
        // 路线全为正交段:锚点应落在某一段上(水平段或垂直段)
        let on_route = e.points.windows(2).any(|w| {
            let (a, b) = (w[0], w[1]);
            let on_horizontal = (anchor.y - a.y).abs() < 0.5
                && (anchor.y - b.y).abs() < 0.5
                && anchor.x >= a.x.min(b.x) - 0.5
                && anchor.x <= a.x.max(b.x) + 0.5;
            let on_vertical = (anchor.x - a.x).abs() < 0.5
                && (anchor.x - b.x).abs() < 0.5
                && anchor.y >= a.y.min(b.y) - 0.5
                && anchor.y <= a.y.max(b.y) + 0.5;
            on_horizontal || on_vertical
        });
        assert!(on_route, "锚点 {anchor:?} 应落在路线 {:?} 上", e.points);
        assert!(e.label_anchor.is_some() == e.label.is_some());
    }

    #[test]
    fn edge_styles_and_arrow_flags_map() {
        let d = render("flowchart TD\nA --- B\nB -.-> C\nC ==> D\nD --> E").unwrap();
        let expect = [
            ("A", "B", LineStyle::Solid, false),
            ("B", "C", LineStyle::Dashed, true),
            ("C", "D", LineStyle::Thick, true),
            ("D", "E", LineStyle::Solid, true),
        ];
        for (e, (from, to, style, arrow)) in d.edges.iter().zip(expect) {
            assert_eq!(
                (e.from.as_str(), e.to.as_str(), e.style, e.arrow),
                (from, to, style, arrow)
            );
        }
        // --- 无箭头但仍按书写方向分层
        assert!(center(&d, "B").y > center(&d, "A").y);
    }

    #[test]
    fn chained_edges_split_into_edges() {
        let d = render("flowchart TD\nA --> B --> C").unwrap();
        assert_eq!(d.nodes.len(), 3);
        assert_eq!(d.edges.len(), 2);
        assert_eq!(
            d.edges
                .iter()
                .map(|e| (e.from.as_str(), e.to.as_str()))
                .collect::<Vec<_>>(),
            vec![("A", "B"), ("B", "C")]
        );
    }

    #[test]
    fn cycle_err_is_readable() {
        let err = render("flowchart TD\na-->b\nb-->a").unwrap_err();
        assert!(err.message.contains("环"), "{err}");
        assert!(err.message.contains("a → b → a"), "{err}");
    }

    #[test]
    fn empty_diagram_err_is_readable() {
        let err = render("flowchart TD\n").unwrap_err();
        assert!(err.message.contains("空"), "{err}");
        let err = render("").unwrap_err();
        assert!(err.message.contains("声明头"), "{err}");
    }

    #[test]
    fn isolated_single_node_renders_without_panic() {
        let d = render("flowchart TD\nA[唯一]").unwrap();
        assert_eq!(d.nodes.len(), 1);
        let r = node_rect(&d, "A");
        assert!(d.bounds.w >= r.size.w && d.bounds.h >= r.size.h);
        // 无边:不 panic、不出边指令
        assert!(d.edges.is_empty());
    }

    #[test]
    fn large_chain_and_branches_lay_out_without_panic() {
        // 60 节点链 + 每三级一条跨层长边:迭代 DFS/Kahn 不随深度递归,
        // 排序与坐标均为多项式;断言计数、层向与无重叠。
        let mut src = String::from("flowchart LR\n");
        for i in 0..60 {
            if i % 3 == 2 {
                src.push_str(&format!("n{} ==>|跳| n{}\n", i - 2, i));
            } else {
                src.push_str(&format!("n{} --> n{}\n", i, i + 1));
            }
        }
        let d = render(&src).unwrap();
        assert_eq!(d.nodes.len(), 60);
        assert!(d.edges.len() >= 59);
        assert!(d
            .edges
            .iter()
            .all(|e| center(&d, &e.to).x > center(&d, &e.from).x));
        for (i, a) in d.nodes.iter().enumerate() {
            for b in d.nodes.iter().skip(i + 1) {
                assert!(!rects_overlap(a.rect, b.rect), "{} 与 {} 重叠", a.id, b.id);
            }
        }
    }

    #[test]
    fn disconnected_components_do_not_overlap() {
        let d = render("flowchart TD\nA --> B\nC --> D").unwrap();
        assert_eq!(d.nodes.len(), 4);
        for (i, a) in d.nodes.iter().enumerate() {
            for b in d.nodes.iter().skip(i + 1) {
                assert!(!rects_overlap(a.rect, b.rect), "{} 与 {} 重叠", a.id, b.id);
            }
        }
    }

    #[test]
    fn cjk_labels_count_double_width() {
        let d = render("flowchart TD\nA[中文字符串]\nB[abcdefgh]").unwrap();
        // 5 个 CJK = 10 个半角单位,应宽于 8 个半角字符
        assert!(
            node_rect(&d, "A").size.w > node_rect(&d, "B").size.w,
            "{:?}",
            d.nodes
        );
    }

    struct FixedMeasures(Size);

    impl TextMeasurer for FixedMeasures {
        fn measure(&self, _text: &str) -> Size {
            self.0
        }
    }

    #[test]
    fn text_measurer_callback_is_injectable() {
        let m = FixedMeasures(Size { w: 100.0, h: 10.0 });
        let d = render_with("flowchart TD\nA[随便]", &m).unwrap();
        let r = node_rect(&d, "A");
        assert_eq!(r.size.w, 100.0 + 2.0 * PAD_X);
        assert_eq!(r.size.h, 10.0 + 2.0 * PAD_Y);
        // 菱形按两倍内接盒放大
        let d2 = render_with("flowchart TD\nA{随便}", &m).unwrap();
        assert_eq!(node_rect(&d2, "A").size.w, 2.0 * (100.0 + PAD_X));
        assert_eq!(node_rect(&d2, "A").size.h, 2.0 * (10.0 + PAD_Y));
    }

    #[test]
    fn unsupported_inputs_error_readably() {
        let cases = [
            "sequenceDiagram\nA->>B: hi",
            "gantt\n    dateFormat YYYY-MM-DD",
            "flowchart TD\nsubgraph 组\nA --> B\nend",
            "flowchart TD\nstyle A fill:#f9f",
            "flowchart TD\nclassDef cls fill:#f9f",
            "flowchart TD\nlinkStyle 0 stroke:#f00",
            "flowchart TD\nA --> B & C",
            "flowchart TD\nA ----> B",
            "flowchart TD\nA -- 文字 --> B",
            "flowchart TD\nA((圆))",
            "flowchart TD\nA[(数据库)]",
            "flowchart TB\nA --> B",
            "flowchart TD extra\nA --> B",
            "flowchart\nA --> B",
            "flowchart TD\nA[未闭合 --> B",
            "flowchart TD\nA -->|未闭合 B",
            "flowchart TD\nA --> B 尾随",
        ];
        for src in cases {
            let err = render(src).expect_err(&format!("{src} 应解析失败"));
            assert!(!err.message.is_empty(), "{src} 的错误信息不应为空");
            assert!(err.to_string().starts_with("mermaid"), "{err}");
        }
    }

    #[test]
    fn error_display_carries_line_number() {
        let err = render("flowchart TD\nA --> B\nA[未闭合").unwrap_err();
        assert_eq!(err.line, 3);
        assert_eq!(err.to_string(), format!("mermaid 第 3 行:{}", err.message));
    }
}
