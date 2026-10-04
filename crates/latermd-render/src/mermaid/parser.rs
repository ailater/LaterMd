//! flowchart/graph v1 子集的行式解析器。
//!
//! 只认任务钉死的语法:声明头(flowchart/graph + TD|LR)、三种节点形状
//! (`A[标签]` / `A(标签)` / `A{标签}`,标签可用双引号包裹以容纳特殊字符)、
//! 四种连线(`---` 无向 / `-->` 实线箭头 / `-.->` 虚线 / `==>` 粗线,可带
//! `|文字|` 标签)、`%%` 注释行与空行。其余一切(其他图类型、subgraph/
//! classDef/style/linkStyle、更长横线的连线、`-- 文字 --` 边标签、`&`
//! 并列、复合形状等)显式返回 [`MermaidError`],由上层整体回落为源码
//! 高亮代码块——不猜测、不吞错。

use super::{Direction, LineStyle, MermaidError, Shape};

/// 一张解析后的 flowchart。
#[derive(Debug, Clone, PartialEq)]
pub struct Flowchart {
    /// 声明头指定的方向。
    pub direction: Direction,
    /// 全部节点(含边里隐式引用的),按首次出现顺序;同 ID 后出现的
    /// 显式声明(标签/形状)覆盖先前的值。
    pub nodes: Vec<NodeDecl>,
    /// 全部边,按出现顺序。
    pub edges: Vec<EdgeDecl>,
}

/// 节点声明。
#[derive(Debug, Clone, PartialEq)]
pub struct NodeDecl {
    /// 节点 ID(源文本原文)。
    pub id: String,
    /// 显示标签;隐式声明的节点以 ID 为标签。
    pub label: String,
    /// 形状。
    pub shape: Shape,
}

/// 边声明。
#[derive(Debug, Clone, PartialEq)]
pub struct EdgeDecl {
    /// 起点 ID。
    pub from: String,
    /// 终点 ID。
    pub to: String,
    /// 线型。
    pub style: LineStyle,
    /// 终点端是否画箭头(`---` 无向为 false)。
    pub arrow: bool,
    /// `|文字|` 边标签(空管道 `||` 视为无标签)。
    pub label: Option<String>,
}

/// v1 显式拒绝的其他 mermaid 图类型(用于给出更准确的错误信息)。
const OTHER_DIAGRAM_TYPES: [&str; 17] = [
    "sequenceDiagram",
    "classDiagram",
    "stateDiagram",
    "stateDiagram-v2",
    "erDiagram",
    "journey",
    "gantt",
    "pie",
    "mindmap",
    "timeline",
    "gitGraph",
    "quadrantChart",
    "requirementDiagram",
    "sankey-beta",
    "xychart-beta",
    "block-beta",
    "packet-beta",
];

/// v1 显式拒绝的 flowchart 内语句关键字。
const RESERVED_STATEMENTS: [&str; 10] = [
    "subgraph",
    "end",
    "classDef",
    "class",
    "style",
    "linkStyle",
    "click",
    "direction",
    "loop",
    "rect",
];

/// 解析 mermaid 源文本为 flowchart AST。语法超出 v1 子集时返回带行号
/// 的可读错误;声明头之后没有任何节点或边也视为错误(空图)。
pub fn parse(source: &str) -> Result<Flowchart, MermaidError> {
    let source = source.trim_start_matches('\u{feff}');
    let mut flowchart: Option<(usize, Flowchart)> = None; // (声明头行号, 图)
    for (idx, raw) in source.split('\n').enumerate() {
        let line_no = idx + 1;
        let line = raw.trim_end_matches('\r');
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("%%") {
            continue;
        }
        match flowchart.as_mut() {
            None => flowchart = Some((line_no, parse_header(line, line_no)?)),
            Some((_, fc)) => parse_statement(fc, line, line_no)?,
        }
    }
    let (header_line, fc) = flowchart.ok_or_else(|| MermaidError {
        line: 1,
        message: "图为空:缺少 flowchart/graph 声明头".into(),
    })?;
    if fc.nodes.is_empty() {
        return Err(MermaidError {
            line: header_line,
            message: "图为空:声明头之后没有任何节点或边".into(),
        });
    }
    Ok(fc)
}

/// 声明头:`flowchart|graph TD|LR`,其后只允许空白或 `%%` 注释。
fn parse_header(line: &str, line_no: usize) -> Result<Flowchart, MermaidError> {
    let err = |message: String| MermaidError {
        line: line_no,
        message,
    };
    let mut cur = Cursor::new(line);
    let keyword = cur.take_while(|c| !c.is_whitespace());
    if OTHER_DIAGRAM_TYPES.contains(&keyword.as_str()) {
        return Err(err(format!(
            "不支持的图表类型「{keyword}」,v1 仅支持 flowchart/graph TD|LR"
        )));
    }
    if keyword != "flowchart" && keyword != "graph" {
        return Err(err(format!(
            "缺少 flowchart/graph 声明头(识别到「{keyword}」)"
        )));
    }
    cur.skip_ws();
    let dir = cur.take_while(is_id_char);
    let direction = match dir.as_str() {
        "TD" => Direction::TopDown,
        "LR" => Direction::LeftRight,
        "" => return Err(err("缺少方向声明,仅支持 TD 或 LR".into())),
        other => return Err(err(format!("方向「{other}」不在 v1 子集,仅支持 TD|LR"))),
    };
    cur.skip_ws();
    let rest = cur.rest().trim();
    if !rest.is_empty() && !rest.starts_with("%%") {
        return Err(err(format!("声明头之后有多余内容「{rest}」")));
    }
    Ok(Flowchart {
        direction,
        nodes: Vec::new(),
        edges: Vec::new(),
    })
}

/// 语句 = 节点(可带形状)与连线的链式序列:`A --> B --> C` 会拆成两条边。
fn parse_statement(fc: &mut Flowchart, line: &str, line_no: usize) -> Result<(), MermaidError> {
    let err = |message: String| MermaidError {
        line: line_no,
        message,
    };
    let first = line.split_whitespace().next().unwrap_or_default();
    if RESERVED_STATEMENTS.contains(&first) {
        return Err(err(format!(
            "不支持的关键字「{first}」:v1 不支持 subgraph/classDef/style/linkStyle 等语句"
        )));
    }
    let mut cur = Cursor::new(line);
    let mut prev = node_ref(fc, &mut cur, line_no)?;
    loop {
        cur.skip_ws();
        if cur.at_end() {
            return Ok(());
        }
        let token = cur.take_while(|c| matches!(c, '-' | '.' | '=' | '>'));
        let (style, arrow) = match token.as_str() {
            "---" => (LineStyle::Solid, false),
            "-->" => (LineStyle::Solid, true),
            "-.->" => (LineStyle::Dashed, true),
            "==>" => (LineStyle::Thick, true),
            "" => {
                return Err(err(format!(
                    "无法识别的语句片段「{}」(期望 --- / --> / -.-> / ==> 或行尾)",
                    cur.rest().trim()
                )));
            }
            other => {
                return Err(err(format!(
                    "不支持的连线语法「{other}」,v1 仅支持 --- / --> / -.-> / ==>"
                )));
            }
        };
        cur.skip_ws();
        let label = if cur.peek() == Some('|') {
            let text = take_pipe_label(&mut cur, line_no)?;
            (!text.is_empty()).then_some(text)
        } else {
            None
        };
        let next = node_ref(fc, &mut cur, line_no)?;
        fc.edges.push(EdgeDecl {
            from: fc.nodes[prev].id.clone(),
            to: fc.nodes[next].id.clone(),
            style,
            arrow,
            label,
        });
        prev = next;
    }
}

/// 解析一个节点引用:ID + 可选形状标签;首次出现则登记(隐式声明以
/// ID 为标签、圆角矩形为形状),再次显式出现则覆盖标签/形状。
fn node_ref(
    fc: &mut Flowchart,
    cur: &mut Cursor<'_>,
    line_no: usize,
) -> Result<usize, MermaidError> {
    let err = |message: String| MermaidError {
        line: line_no,
        message,
    };
    cur.skip_ws();
    let id = cur.take_while(is_id_char);
    if id.is_empty() {
        let at = if cur.at_end() {
            "行尾".to_owned()
        } else {
            cur.rest().trim().to_owned()
        };
        return Err(err(format!("缺少节点 ID(识别到「{at}」)")));
    }
    cur.skip_ws();
    let spec = match cur.peek() {
        Some('[') => Some(shape_label(cur, '[', ']', line_no)?),
        Some('(') => Some(shape_label(cur, '(', ')', line_no)?),
        Some('{') => Some(shape_label(cur, '{', '}', line_no)?),
        _ => None,
    };
    Ok(match fc.nodes.iter().position(|n| n.id == id) {
        Some(i) => {
            if let Some((label, shape)) = spec {
                fc.nodes[i].label = label;
                fc.nodes[i].shape = shape;
            }
            i
        }
        None => {
            let (label, shape) = spec.unwrap_or_else(|| (id.clone(), Shape::RoundedRect));
            fc.nodes.push(NodeDecl {
                id: id.clone(),
                label,
                shape,
            });
            fc.nodes.len() - 1
        }
    })
}

/// 解析形状标签:先按定界符扫到闭合再做字符校验,保证「未闭合」优先
/// 报缺右定界符;双引号标签可容纳 `)`、`<` 等特殊字符。
fn shape_label(
    cur: &mut Cursor<'_>,
    open: char,
    close: char,
    line_no: usize,
) -> Result<(String, Shape), MermaidError> {
    let err = |message: String| MermaidError {
        line: line_no,
        message,
    };
    cur.bump(); // open
    if cur.peek() == Some(open) || matches!(cur.peek(), Some('[' | '(' | '{')) {
        return Err(err(
            "不支持的复合节点形状:v1 仅支持 A[标签] / A(标签) / A{标签} 三种".into(),
        ));
    }
    let label = if cur.peek() == Some('"') {
        cur.bump();
        let start = cur.pos;
        while let Some(c) = cur.peek() {
            if c == '"' {
                break;
            }
            cur.bump();
        }
        if cur.peek() != Some('"') {
            return Err(err("标签的双引号未闭合".into()));
        }
        let text = cur.text[start..cur.pos].to_owned();
        cur.bump(); // '"'
        if cur.peek() != Some(close) {
            return Err(err(format!("引号标签之后应紧跟「{close}」")));
        }
        cur.bump(); // close
        text
    } else {
        let start = cur.pos;
        while let Some(c) = cur.peek() {
            if c == close {
                break;
            }
            cur.bump();
        }
        if cur.peek() != Some(close) {
            return Err(err(format!("标签缺少右定界符「{close}」")));
        }
        let text = &cur.text[start..cur.pos];
        if text.contains('<') || text.contains('"') {
            return Err(err(
                "标签中含有 \" 或 <:请改用双引号标签(不支持 <br/> 等内联标记)".into(),
            ));
        }
        let text = text.trim().to_owned();
        cur.bump(); // close
        text
    };
    let shape = match open {
        '[' => Shape::RoundedRect,
        '(' => Shape::Stadium,
        _ => Shape::Diamond,
    };
    Ok((label, shape))
}

/// 解析 `|文字|` 边标签(光标已停在左竖线)。
fn take_pipe_label(cur: &mut Cursor<'_>, line_no: usize) -> Result<String, MermaidError> {
    cur.bump(); // '|'
    let start = cur.pos;
    while let Some(c) = cur.peek() {
        if c == '|' {
            break;
        }
        cur.bump();
    }
    if cur.peek() != Some('|') {
        return Err(MermaidError {
            line: line_no,
            message: "边标签缺少右竖线 |".into(),
        });
    }
    let text = cur.text[start..cur.pos].trim().to_owned();
    cur.bump(); // '|'
    Ok(text)
}

/// ID 合法字符:非空白,且不在保留符号集(形状定界/连线/标签/并列等)。
fn is_id_char(c: char) -> bool {
    !c.is_whitespace() && !"[](){}<>=|-&;\"%'#,`".contains(c)
}

/// 行内光标:按字符边界推进的轻量扫描器。
struct Cursor<'a> {
    text: &'a str,
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(text: &'a str) -> Self {
        Self { text, pos: 0 }
    }

    fn peek(&self) -> Option<char> {
        self.text[self.pos..].chars().next()
    }

    fn bump(&mut self) {
        if let Some(c) = self.peek() {
            self.pos += c.len_utf8();
        }
    }

    fn skip_ws(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.bump();
        }
    }

    fn at_end(&self) -> bool {
        self.pos >= self.text.len()
    }

    fn rest(&self) -> &'a str {
        &self.text[self.pos..]
    }

    fn take_while(&mut self, mut keep: impl FnMut(char) -> bool) -> String {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if !keep(c) {
                break;
            }
            self.bump();
        }
        self.text[start..self.pos].to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find<'a>(fc: &'a Flowchart, id: &str) -> &'a NodeDecl {
        fc.nodes.iter().find(|n| n.id == id).unwrap()
    }

    #[test]
    fn header_directions() {
        assert_eq!(
            parse("flowchart TD\nA").unwrap().direction,
            Direction::TopDown
        );
        assert_eq!(
            parse("flowchart LR\nA").unwrap().direction,
            Direction::LeftRight
        );
        assert_eq!(parse("graph TD\nA").unwrap().direction, Direction::TopDown);
        assert_eq!(
            parse("graph LR\nA").unwrap().direction,
            Direction::LeftRight
        );
    }

    #[test]
    fn shapes_labels_and_implicit_nodes() {
        let fc = parse("flowchart TD\nA[矩形] --> B(跑道)\nB --> C{菱形}\nC --> D\nD[\"带)括号\"]")
            .unwrap();
        assert_eq!(fc.nodes.len(), 4);
        assert_eq!(
            (find(&fc, "A").shape, find(&fc, "A").label.as_str()),
            (Shape::RoundedRect, "矩形")
        );
        assert_eq!(
            (find(&fc, "B").shape, find(&fc, "B").label.as_str()),
            (Shape::Stadium, "跑道")
        );
        assert_eq!(
            (find(&fc, "C").shape, find(&fc, "C").label.as_str()),
            (Shape::Diamond, "菱形")
        );
        // D 先被 C --> D 隐式引用(默认标签=ID、圆角矩形),后显式声明覆盖
        assert_eq!(fc.nodes.iter().position(|n| n.id == "D").unwrap(), 3);
        assert_eq!(
            (find(&fc, "D").shape, find(&fc, "D").label.as_str()),
            (Shape::RoundedRect, "带)括号")
        );
    }

    #[test]
    fn edge_kinds_labels_and_chains() {
        let fc = parse(
            "flowchart TD\nA --- B\nB -->|普通| C\nC -.-> D\nD ==>|粗| E\nF --> G --> H\nI -->|| J",
        )
        .unwrap();
        assert_eq!(fc.edges.len(), 7);
        let expect = [
            ("A", "B", LineStyle::Solid, false, None),
            ("B", "C", LineStyle::Solid, true, Some("普通")),
            ("C", "D", LineStyle::Dashed, true, None),
            ("D", "E", LineStyle::Thick, true, Some("粗")),
            ("F", "G", LineStyle::Solid, true, None),
            ("G", "H", LineStyle::Solid, true, None),
            ("I", "J", LineStyle::Solid, true, None), // 空管道视为无标签
        ];
        for (e, (from, to, style, arrow, label)) in fc.edges.iter().zip(expect) {
            assert_eq!(
                (
                    e.from.as_str(),
                    e.to.as_str(),
                    e.style,
                    e.arrow,
                    e.label.as_deref()
                ),
                (from, to, style, arrow, label)
            );
        }
    }

    #[test]
    fn explicit_redeclaration_overrides() {
        let fc = parse("flowchart TD\nA --> B[x]\nB[y]\nA(甲)").unwrap();
        assert_eq!(find(&fc, "B").label, "y");
        assert_eq!(find(&fc, "A").label, "甲");
        assert_eq!(find(&fc, "A").shape, Shape::Stadium);
        // 重复显式声明不新增节点
        assert_eq!(fc.nodes.len(), 2);
    }

    #[test]
    fn comments_blanks_and_crlf_tolerated() {
        let src = "%%{init}%%\r\n\r\nflowchart TD %% 行尾注释\r\n%% 中间注释\r\nA --> B\r\n";
        let fc = parse(src).unwrap();
        assert_eq!((fc.nodes.len(), fc.edges.len()), (2, 1));
    }

    #[test]
    fn syntax_errors_carry_line_numbers() {
        let err = parse("flowchart TD\nA --> B\nA[未闭合").unwrap_err();
        assert_eq!(err.line, 3);
        assert!(err.message.contains(']'), "{err}");
        let err = parse("flowchart TD\nA --> B\nB -->|漏了竖线 C").unwrap_err();
        assert_eq!(err.line, 3);
        assert!(err.message.contains("竖线"), "{err}");
    }
}
