//! Sugiyama-lite 分层布局。
//!
//! 步骤:环检测(迭代 DFS,发现环返回 [`MermaidError`] 不 panic)→
//! Kahn 拓扑序上的最长路径分层(每条边满足 layer(v) ≥ layer(u)+1)→
//! 跨层长边拆**虚拟节点**(零尺寸折点,参与排序与占位)→ 层内
//! barycenter 排序消交叉(下行+上行往返,重心并列时保持现有相对序,
//! 结果确定)→ 层内按盒宽打包居中、层间按最大行高对齐的坐标分配。
//! LR 方向在 TD 坐标系算完后整体转置。
//!
//! 连线取正交肘形:相邻两层的路径点之间「垂直下探到间隙带中线 →
//! 水平横移 → 垂直下探」。虚拟节点让每条边都只走相邻层,因此折线
//! 不穿任何盒(垂直段始终位于路径点自己的列槽内,水平段始终位于
//! 两层之间的间隙带内)。布局参数全部是具名常量,观感校准改常量即可。

use std::collections::HashMap;

use super::parser::{Flowchart, NodeDecl};
use super::{
    ColorSlot, Diagram, Direction, EdgePath, MermaidError, NodeDraw, Point, Rect, Shape, Size,
    TextMeasurer, ZLayer,
};

/// 层内相邻盒的间距(px)。
pub const NODE_GAP: f32 = 28.0;
/// 相邻两层的行间距(px)。
pub const LAYER_GAP: f32 = 56.0;
/// 盒内水平内边距(px)。
pub const PAD_X: f32 = 14.0;
/// 盒内垂直内边距(px)。
pub const PAD_Y: f32 = 9.0;
/// 半角字符宽(px,默认字符计数测量用)。
pub const CHAR_W: f32 = 8.5;
/// 单行文字高(px,默认字符计数测量用)。
pub const LINE_H: f32 = 22.0;
/// barycenter 排序的下行+上行往返次数。
const ORDER_ROUNDS: usize = 2;

/// 分层布局 flowchart AST,产出纯几何绘制指令。图里有环时返回错误
/// (消息含环路径),不 panic。
pub fn layout(fc: &Flowchart, measure: &dyn TextMeasurer) -> Result<Diagram, MermaidError> {
    let n = fc.nodes.len();
    let mut index: HashMap<&str, usize> = HashMap::with_capacity(n);
    for (i, node) in fc.nodes.iter().enumerate() {
        index.insert(node.id.as_str(), i);
    }

    // —— 真实边邻接:环检测与分层都用它(虚拟节点只是边的细分,不改环性)——
    let mut real_out: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut indeg = vec![0usize; n];
    for edge in &fc.edges {
        let (u, v) = (index[edge.from.as_str()], index[edge.to.as_str()]);
        real_out[u].push(v);
        indeg[v] += 1;
    }

    detect_cycle(fc, &real_out)?;

    // Kahn 拓扑序上做最长路径分层(`---` 无向边也按书写方向参与分层,
    // 只是绘制时不带箭头;若按无向等价处理,任何 A --- B 都会成环)。
    let mut layers = vec![0usize; n];
    let mut pending = indeg.clone();
    let mut ready: Vec<usize> = (0..n).filter(|&i| pending[i] == 0).collect();
    let mut topo = Vec::with_capacity(n);
    while let Some(u) = ready.pop() {
        topo.push(u);
        for &v in &real_out[u] {
            layers[v] = layers[v].max(layers[u] + 1);
            pending[v] -= 1;
            if pending[v] == 0 {
                ready.push(v);
            }
        }
    }
    debug_assert_eq!(topo.len(), n, "detect_cycle 已排除环,拓扑序应为全量");
    let layer_count = layers.iter().copied().max().unwrap_or(0) + 1;

    // —— 跨层长边拆虚拟节点:每跨一层补一个零尺寸折点 ——
    // 组合节点空间:0..n 为真实节点,n.. 为虚拟节点。
    let mut ext_layers = layers.clone();
    let mut chains: Vec<Vec<usize>> = Vec::with_capacity(fc.edges.len());
    for edge in &fc.edges {
        let (u, v) = (index[edge.from.as_str()], index[edge.to.as_str()]);
        let mut chain = vec![u];
        for l in layers[u] + 1..layers[v] {
            ext_layers.push(l);
            chain.push(ext_layers.len() - 1);
        }
        chain.push(v);
        chains.push(chain);
    }
    let total = ext_layers.len();

    let mut out_adj: Vec<Vec<usize>> = vec![Vec::new(); total];
    let mut in_adj: Vec<Vec<usize>> = vec![Vec::new(); total];
    for chain in &chains {
        for w in chain.windows(2) {
            out_adj[w[0]].push(w[1]);
            in_adj[w[1]].push(w[0]);
        }
    }

    let mut by_layer: Vec<Vec<usize>> = vec![Vec::new(); layer_count];
    for i in 0..total {
        by_layer[ext_layers[i]].push(i);
    }

    // —— barycenter 消交叉 ——
    let mut pos = vec![0usize; total];
    for layer in &by_layer {
        for (i, &node) in layer.iter().enumerate() {
            pos[node] = i;
        }
    }
    for _ in 0..ORDER_ROUNDS {
        // 下行:按上一层邻居重心排;上行:按下一层邻居重心排
        for (k, layer) in by_layer.iter_mut().enumerate().skip(1) {
            order_layer(layer, &mut pos, &in_adj, &ext_layers, k - 1);
        }
        for (k, layer) in by_layer.iter_mut().enumerate().rev().skip(1) {
            order_layer(layer, &mut pos, &out_adj, &ext_layers, k + 1);
        }
    }

    // —— 尺寸与行几何(虚拟节点零尺寸:不占行高,占一个列槽)——
    let mut sizes: Vec<Size> = fc.nodes.iter().map(|nd| node_size(nd, measure)).collect();
    sizes.resize(total, Size { w: 0.0, h: 0.0 });
    let mut row_h = vec![0f32; layer_count];
    for (i, size) in sizes.iter().enumerate() {
        row_h[ext_layers[i]] = row_h[ext_layers[i]].max(size.h);
    }
    let mut y_center = vec![0f32; layer_count];
    let mut cursor = 0f32;
    for (k, yc) in y_center.iter_mut().enumerate() {
        *yc = cursor + row_h[k] / 2.0;
        cursor += row_h[k] + LAYER_GAP;
    }
    let total_h = cursor - LAYER_GAP;

    let mut layer_w = vec![0f32; layer_count];
    for (k, layer) in by_layer.iter().enumerate() {
        let gaps = NODE_GAP * layer.len().saturating_sub(1) as f32;
        layer_w[k] = layer.iter().map(|&i| sizes[i].w).sum::<f32>() + gaps;
    }
    let max_w = layer_w.iter().copied().fold(0f32, f32::max);

    let mut rects = vec![Rect::default(); total];
    for (k, layer) in by_layer.iter().enumerate() {
        let mut x = (max_w - layer_w[k]) / 2.0;
        for &node in layer {
            rects[node] = Rect {
                min: Point {
                    x,
                    y: y_center[k] - sizes[node].h / 2.0,
                },
                size: sizes[node],
            };
            x += sizes[node].w + NODE_GAP;
        }
    }
    let row_top: Vec<f32> = (0..layer_count)
        .map(|k| y_center[k] - row_h[k] / 2.0)
        .collect();
    let row_bottom: Vec<f32> = (0..layer_count)
        .map(|k| y_center[k] + row_h[k] / 2.0)
        .collect();

    // —— 连线:沿链逐对相邻层走正交肘形 ——
    let mut edges: Vec<EdgePath> = Vec::with_capacity(fc.edges.len());
    for (edge, chain) in fc.edges.iter().zip(&chains) {
        let u = chain[0];
        let v = chain[chain.len() - 1];
        let start = Point {
            x: rects[u].center().x,
            y: rects[u].max_y(),
        };
        let end = Point {
            x: rects[v].center().x,
            y: rects[v].min.y,
        };
        let mut points = vec![start];
        let mut prev = (start, ext_layers[u]);
        for &w in &chain[1..chain.len() - 1] {
            let wc = rects[w].center();
            elbow(&mut points, prev, wc, &row_top, &row_bottom);
            points.push(wc);
            prev = (wc, ext_layers[w]);
        }
        elbow(&mut points, prev, end, &row_top, &row_bottom);
        points.push(end);
        let label_anchor = edge.label.as_ref().map(|_| route_midpoint(&points));
        edges.push(EdgePath {
            from: edge.from.clone(),
            to: edge.to.clone(),
            style: edge.style,
            arrow: edge.arrow,
            points,
            label: edge.label.clone(),
            label_anchor,
            stroke: ColorSlot::Line,
            z: ZLayer::Edge,
        });
    }

    let nodes: Vec<NodeDraw> = fc
        .nodes
        .iter()
        .enumerate()
        .map(|(i, nd)| NodeDraw {
            id: nd.id.clone(),
            label: nd.label.clone(),
            shape: nd.shape,
            rect: rects[i],
            text_anchor: rects[i].center(),
            fill: ColorSlot::NodeFill,
            stroke: ColorSlot::NodeStroke,
            z: ZLayer::Node,
        })
        .collect();

    // —— LR:整体转置(x/y 互换) ——
    let (bounds, nodes, edges) = if fc.direction == Direction::LeftRight {
        (
            Size {
                w: total_h,
                h: max_w,
            },
            nodes
                .into_iter()
                .map(|mut node| {
                    node.rect = transpose_rect(node.rect);
                    node.text_anchor = transpose(node.text_anchor);
                    node
                })
                .collect(),
            edges
                .into_iter()
                .map(|mut edge| {
                    edge.points = edge.points.iter().map(|p| transpose(*p)).collect();
                    edge.label_anchor = edge.label_anchor.map(transpose);
                    edge
                })
                .collect(),
        )
    } else {
        (
            Size {
                w: max_w,
                h: total_h,
            },
            nodes,
            edges,
        )
    };

    Ok(Diagram {
        direction: fc.direction,
        bounds,
        nodes,
        edges,
    })
}

/// 往折线里补一对肘形拐点:从 `prev`(含所在层)下探到 `next` 所在层
/// 上方的间隙带中线再横移。同列时无需拐点(直接垂直)。
fn elbow(
    points: &mut Vec<Point>,
    prev: (Point, usize),
    next: Point,
    row_top: &[f32],
    row_bottom: &[f32],
) {
    if (prev.0.x - next.x).abs() <= 0.5 {
        return;
    }
    let gap_y = (row_bottom[prev.1] + row_top[prev.1 + 1]) / 2.0;
    points.push(Point {
        x: prev.0.x,
        y: gap_y,
    });
    points.push(Point {
        x: next.x,
        y: gap_y,
    });
}

/// 折线按累计长度的中点(边标签锚点)。
fn route_midpoint(points: &[Point]) -> Point {
    let mut acc = vec![0f32];
    let mut total = 0f32;
    for w in points.windows(2) {
        total += (w[1].x - w[0].x).hypot(w[1].y - w[0].y);
        acc.push(total);
    }
    if total <= 0.0 {
        return points[0];
    }
    let half = total / 2.0;
    for (i, w) in points.windows(2).enumerate() {
        if acc[i + 1] >= half {
            let span = acc[i + 1] - acc[i];
            let t = if span == 0.0 {
                0.0
            } else {
                (half - acc[i]) / span
            };
            return Point {
                x: w[0].x + (w[1].x - w[0].x) * t,
                y: w[0].y + (w[1].y - w[0].y) * t,
            };
        }
    }
    points[points.len() - 1]
}

/// 环检测:迭代 DFS(显式栈,不随图深递归)。发现环时把环路径写进
/// 错误消息(如 `A → B → A`)。
fn detect_cycle(fc: &Flowchart, out_adj: &[Vec<usize>]) -> Result<(), MermaidError> {
    let n = fc.nodes.len();
    let mut color = vec![0u8; n]; // 0 未访 / 1 在途 / 2 完成
    for start in 0..n {
        if color[start] != 0 {
            continue;
        }
        let mut stack: Vec<(usize, usize)> = vec![(start, 0)];
        let mut path: Vec<usize> = vec![start];
        color[start] = 1;
        while let Some(frame) = stack.last_mut() {
            let node = frame.0;
            if frame.1 < out_adj[node].len() {
                let child = out_adj[node][frame.1];
                frame.1 += 1;
                match color[child] {
                    0 => {
                        color[child] = 1;
                        path.push(child);
                        stack.push((child, 0));
                    }
                    1 => {
                        let pos = path.iter().position(|&x| x == child).unwrap_or(0);
                        let mut names: Vec<&str> = path[pos..]
                            .iter()
                            .map(|&i| fc.nodes[i].id.as_str())
                            .collect();
                        names.push(fc.nodes[child].id.as_str());
                        return Err(MermaidError {
                            line: 0,
                            message: format!(
                                "检测到环:{};v1 布局不支持环,请断开环上的边后再试",
                                names.join(" → ")
                            ),
                        });
                    }
                    _ => {}
                }
            } else {
                color[node] = 2;
                stack.pop();
                path.pop();
            }
        }
    }
    Ok(())
}

/// 层内按 `other` 层邻居重心升序稳定排序;无邻居者保持当前位次。
fn order_layer(
    layer: &mut [usize],
    pos: &mut [usize],
    adj: &[Vec<usize>],
    layers: &[usize],
    other: usize,
) {
    let bary: Vec<f32> = layer
        .iter()
        .map(|&node| {
            let mut sum = 0.0;
            let mut count = 0.0;
            for &neighbor in &adj[node] {
                if layers[neighbor] == other {
                    sum += pos[neighbor] as f32;
                    count += 1.0;
                }
            }
            if count == 0.0 {
                pos[node] as f32
            } else {
                sum / count
            }
        })
        .collect();
    let mut order: Vec<usize> = (0..layer.len()).collect();
    order.sort_by(|&a, &b| {
        bary[a]
            .partial_cmp(&bary[b])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    // sort_by 稳定:重心并列时保持现有相对次序,结果确定
    let sorted: Vec<usize> = order.into_iter().map(|i| layer[i]).collect();
    layer.copy_from_slice(&sorted);
    for (i, &node) in layer.iter().enumerate() {
        pos[node] = i;
    }
}

/// 估算节点尺寸:文字测量 + 内边距;菱形按「容纳两倍文字盒」放大,
/// 保证居中文字落在菱形内部。
fn node_size(node: &NodeDecl, measure: &dyn TextMeasurer) -> Size {
    let text = measure.measure(&node.label);
    match node.shape {
        Shape::Diamond => Size {
            w: 2.0 * (text.w + PAD_X),
            h: 2.0 * (text.h + PAD_Y),
        },
        Shape::RoundedRect | Shape::Stadium => Size {
            w: text.w + 2.0 * PAD_X,
            h: text.h + 2.0 * PAD_Y,
        },
    }
}

/// 转置一点(x/y 互换)。
fn transpose(p: Point) -> Point {
    Point { x: p.y, y: p.x }
}

/// 转置矩形(左上角与宽高同步互换)。
fn transpose_rect(r: Rect) -> Rect {
    Rect {
        min: transpose(r.min),
        size: Size {
            w: r.size.h,
            h: r.size.w,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mermaid::{parse, CharCountMeasurer};

    #[test]
    fn barycenter_reorders_layer_to_remove_crossing() {
        // 声明序 C 在 D 前,A→D、B→C 的初始顺序交叉;
        // 下行 barycenter 应把 D 翻到 C 左侧,两行顺序不再倒置。
        let fc = parse("flowchart TD\nC\nD\nA --> D\nB --> C").unwrap();
        let d = layout(&fc, &CharCountMeasurer).unwrap();
        let cx = |id: &str| d.nodes.iter().find(|n| n.id == id).unwrap().rect.center().x;
        assert!(cx("A") < cx("B"));
        assert!(cx("D") < cx("C"), "D 应被排到 C 左侧以消除交叉");
    }

    #[test]
    fn longest_path_layers_follow_edges() {
        let fc = parse("flowchart TD\nA --> B\nB --> C\nA --> C\nC --> D").unwrap();
        let d = layout(&fc, &CharCountMeasurer).unwrap();
        let cy = |id: &str| d.nodes.iter().find(|n| n.id == id).unwrap().rect.center().y;
        assert!(cy("A") < cy("B"));
        assert!(cy("B") < cy("C"));
        assert!(cy("C") < cy("D"));
    }

    #[test]
    fn cycle_detected_with_path_in_message() {
        let fc = parse("flowchart TD\na --> b\nb --> c\nc --> a").unwrap();
        let err = layout(&fc, &CharCountMeasurer).unwrap_err();
        assert!(err.message.contains("环"), "{err}");
        assert!(err.message.contains("a → b → c → a"), "{err}");
    }

    #[test]
    fn self_loop_is_a_cycle() {
        let fc = parse("flowchart TD\nA --> A").unwrap();
        let err = layout(&fc, &CharCountMeasurer).unwrap_err();
        assert!(err.message.contains("A → A"), "{err}");
    }

    #[test]
    fn long_edge_gets_virtual_waypoint_between_layers() {
        // A→C 跨两层:折线应至少经过一个中间层拐点(虚拟节点),
        // 且全部 y 落在源盒下边缘与目标盒上边缘之间。
        let fc = parse("flowchart TD\nA --> B\nA --> C\nB --> C").unwrap();
        let d = layout(&fc, &CharCountMeasurer).unwrap();
        let long = d
            .edges
            .iter()
            .find(|e| e.from == "A" && e.to == "C")
            .expect("应存在 A→C 边");
        assert!(long.points.len() >= 3, "长边应有中间折点:{:?}", long.points);
        let src_bottom = d.nodes.iter().find(|n| n.id == "A").unwrap().rect.max_y();
        let dst_top = d.nodes.iter().find(|n| n.id == "C").unwrap().rect.min.y;
        for p in &long.points {
            assert!(
                p.y >= src_bottom - 0.01 && p.y <= dst_top + 0.01,
                "折线越界:{:?}",
                long.points
            );
        }
    }
}
