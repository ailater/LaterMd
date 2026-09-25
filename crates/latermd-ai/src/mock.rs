//! `MockProvider`:按固定节奏流出真实感中文 Markdown 续写文本。
//!
//! 用途:无真实 key 时联调全链路(decisions-pending #3)。默认 ~100ms/块,
//! 每块 20-80 字符,总时长 3-5 秒;按 prompt 关键词在两套脚本间选择,
//! 使「块序号」可从 prompt 上下文微调。

use std::sync::mpsc::Sender;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::summary::SUMMARY_INSTRUCTIONS;
use crate::{AiProvider, Chunk};

/// 单块最小字符数。
const MIN_CHUNK: usize = 20;
/// 单块最大字符数。
const MAX_CHUNK: usize = 80;
/// 分块目标字符数(20-80 区间的下沿,保证块数够多、节奏可感知)。
const CHUNK_TARGET: usize = 20;
/// 摘要结果的分块目标字符数(ask 口径 ~60,模拟打字机;尾块并入后仍不破
/// 单块上限:60 + MIN_CHUNK == MAX_CHUNK)。
const SUMMARY_CHUNK_TARGET: usize = 60;
/// 默认发块间隔。
const DEFAULT_INTERVAL: Duration = Duration::from_millis(100);

const SCRIPT_DEFAULT: &str = "\
## 3.2 缓存失效策略

缓存与源数据的一致性是本设计的核心矛盾。我们把常见的三种做法摆在一起比较：定时刷新、读写穿透、写入时失效，最终选择「写入时失效」，理由有三：

- 定时刷新在高峰期会造成缓存击穿，需要额外的互斥与退避逻辑，复杂度与收益不成比例；
- 读写穿透把一致性责任压在读路径上，冷启动的首次读会明显变慢，排查也困难；
- 写入时失效与业务事务天然对齐，回滚时能一并撤销，团队的心智负担最小。

失效入口的参考实现如下，注意调用时机必须放在事务提交之后：

```rust
fn invalidate(cache: &mut Cache, doc_id: DocId) {
    if let Some(entry) = cache.remove(&doc_id) {
        log::debug!(\"invalidated {} ({} bytes)\", doc_id, entry.len());
    }
}
```

若把失效放在提交之前，并发读线程会把旧值重新装回缓存，前面的功夫全部白费。正确的顺序是先提交事务、再触发失效，两者之间允许一个短暂的旧值窗口，由前端的重试策略兜底。

键的设计同样值得交代：失效粒度以文档为最小单位，不引入更细的键前缀，避免枚举失效范围时出错。缓存值附带版本号，读到过期版本直接丢弃重取。

最后是观测：命中率低于八成时要先看键的拼写与淘汰策略，而不是急着加缓存。如果后续压测发现写放大明显，再考虑把失效请求按批次合并，现阶段保持简单，不做过度设计。";

const SCRIPT_CODE: &str = "\
### 接入落地步骤

先把最小闭环跑通，再谈扩展。整个接入分四步，每一步都有可以独立验证的完成标准：

1. 定义 `Provider` trait，只暴露流式补全一个方法，输出收口在 `mpsc::Sender`；
2. 实现 `MockProvider`，按固定节奏吐块，让 UI 层不等真实模型就能联调；
3. 接 OpenAI 兼容端点，SSE 逐行解析，遇到 `[DONE]` 收尾；
4. 文本回写编辑器走 `Message` 归约，后台线程不碰 UI 状态。

第 2 步的核心循环很短，值得整段贴出来：

```rust
for piece in script {
    thread::sleep(interval);
    if tx.send(piece).is_err() {
        return;
    }
}
```

这里有两个容易踩的坑。一是块与块之间必须让出线程，`sleep` 顺便充当节流器，避免 UI 一帧收上百条消息；二是发送失败要立刻退出函数，不能对着已经关闭的通道继续灌数据，否则会悄悄泄漏后台线程。

另外别忘了超时与取消的语义要对齐：取消意味着接收端直接丢弃通道，发送端下一次 `send` 失败后立即收尾，不写任何残余文本；超时则由传输层负责，应用层只认错误块。两条路径都收敛到同一个收尾逻辑，UI 侧就不需要区分处理。

剩下的都是体力活：把错误路径补齐，跑通闭环之后再回头看要不要加重试。原则上第一步定的 trait 不再改签名，后面所有 provider 都往这一个方法里装。";

/// 按 prompt 形态选脚本并分块。摘要请求(以 summary 指令头开头)走同步
/// 合成 + 60 字符分块;含「代码/code/实现」的续写走代码密集脚本。
fn script_for(prompt: &str) -> Vec<String> {
    if prompt.starts_with(SUMMARY_INSTRUCTIONS) {
        return chunk_text(&mock_summary_body(prompt), SUMMARY_CHUNK_TARGET);
    }
    let lower = prompt.to_lowercase();
    let text = if prompt.contains("代码") || prompt.contains("实现") || lower.contains("code") {
        SCRIPT_CODE
    } else {
        SCRIPT_DEFAULT
    };
    chunk_text(text, CHUNK_TARGET)
}

/// 按字符(非字节)分块,尾块不足 [`MIN_CHUNK`] 时并入前一块;
/// 整段文本不足一块时原样成单块。
fn chunk_text(text: &str, target: usize) -> Vec<String> {
    // 尾块并入前一块后仍不得突破单块上限,这要求 target 足够小
    debug_assert!(target + MIN_CHUNK <= MAX_CHUNK, "target 过大将突破单块上限");
    let chars: Vec<char> = text.chars().collect();
    let mut chunks: Vec<String> = chars
        .chunks(target)
        .map(|part| part.iter().collect())
        .collect();
    let needs_merge = chunks.len() > 1
        && chunks
            .last()
            .is_some_and(|last| last.chars().count() < MIN_CHUNK);
    if needs_merge {
        if let Some(last) = chunks.pop() {
            if let Some(prev) = chunks.last_mut() {
                prev.push_str(&last);
            }
        }
    }
    chunks
}

/// 演示用 provider:后台线程按固定间隔吐块,正文拼起来恰好是脚本全文。
#[derive(Clone, Debug)]
pub struct MockProvider {
    interval: Duration,
}

impl Default for MockProvider {
    fn default() -> Self {
        Self {
            interval: DEFAULT_INTERVAL,
        }
    }
}

impl MockProvider {
    pub fn new() -> Self {
        Self::default()
    }

    /// 自定义发块间隔(联调与测试加速用)。
    pub fn with_interval(interval: Duration) -> Self {
        Self { interval }
    }

    /// commit message 场景的同步生成(菜单「AI: 生成 commit message」)。
    ///
    /// 不走 [`AiProvider::stream_complete`]:流式脚本拼起来是续写正文,
    /// 对 commit 场景不适用。真实 key 接入后由网络 provider 的低温度补全
    /// 取代(调方收口,见 app 侧 `request_commit_message` 的 TODO),本方法
    /// 保留作演示与测试。输入是 [`crate::commit_message_prompt`] 的产物,
    /// 按 diff 关键词合成单行 conventional 中文 subject。
    pub fn mock_commit_subject(&self, prompt: &str) -> String {
        commit_subject_for(prompt)
    }

    /// 摘要场景的同步生成(菜单「AI: 生成摘要」背后的 Mock 替身)。
    ///
    /// 与 [`MockProvider::mock_commit_subject`] 对称,输入是
    /// [`crate::summary_prompt`] 的产物;但摘要**走流式通道** ——
    /// [`AiProvider::stream_complete`] 检测到摘要指令头时调同一逻辑并按
    /// 60 字符分块,方法形态只为单测与演示保留。输出是最终文档形态:每条
    /// 要点一行引用块列表(`> - …`),行数 3-5 条。
    pub fn mock_summary(&self, prompt: &str) -> String {
        mock_summary_body(prompt)
    }
}

/// 摘要要点条数区间(ask 口径:3-5 条)。
const MIN_POINTS: usize = 3;
const MAX_POINTS: usize = 5;
/// 单条要点的字符上限(含省略号占一位):摘要行要保持一行可扫读。
const POINT_LIMIT_CHARS: usize = 40;

/// 从摘要 prompt(指令头 + 文档全文)按标题/列表关键词抽要点,拼成引用块
/// 列表。指令头剥掉后逐行扫:标题行与列表行是首选候选;候选不足 3 条时用
/// 段落行补足(演示替身,保证 3-5 条的观感);引用/表格行与围栏代码块内的
/// 行不抽(它们不是文档自己的叙述主线)。空文档由调用方拦截,这里对抽不到
/// 任何文本行的输入给兜底单条,绝不返回空串(流式通道拼空串会没有可感知
/// 的打字机效果)。
fn mock_summary_body(prompt: &str) -> String {
    let body = prompt.strip_prefix(SUMMARY_INSTRUCTIONS).unwrap_or(prompt);
    let mut points: Vec<String> = Vec::new();
    let mut paragraphs: Vec<&str> = Vec::new();
    let mut in_code = false;
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code || trimmed.is_empty() {
            continue;
        }
        if let Some(point) = heading_point(trimmed).or_else(|| list_point(trimmed)) {
            if points.len() < MAX_POINTS {
                points.push(clamp_point(point));
            }
        } else if !trimmed.starts_with('|') && !trimmed.starts_with('>') {
            paragraphs.push(trimmed);
        }
    }
    for paragraph in paragraphs {
        if points.len() >= MIN_POINTS {
            break;
        }
        points.push(clamp_point(paragraph));
    }
    if points.is_empty() {
        points.push("文档内容太少,没有可提炼的要点".to_owned());
    }
    let mut out = String::new();
    for point in points {
        out.push_str("> - ");
        out.push_str(&point);
        out.push('\n');
    }
    out
}

/// ATX 标题行(`#`×1-6 + 空格)剥掉标记取标题文本;非标题行返回 `None`。
fn heading_point(line: &str) -> Option<&str> {
    let hashes = line.chars().take_while(|&c| c == '#').count();
    if !(1..=6).contains(&hashes) {
        return None; // 0 个不是标题,7 个以上按 CommonMark 也不是
    }
    let text = line.get(hashes..)?.strip_prefix(' ')?.trim();
    (!text.is_empty()).then_some(text)
}

/// 无序/有序列表行剥掉标记取项目文本(任务标记 `[ ]`/`[x]` 一并剥掉);
/// 非列表行返回 `None`。
fn list_point(line: &str) -> Option<&str> {
    for marker in ["- ", "* ", "+ "] {
        if let Some(rest) = line.strip_prefix(marker) {
            return Some(strip_task_marker(rest));
        }
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 && line[digits..].starts_with(". ") {
        return Some(strip_task_marker(&line[digits + 2..]));
    }
    None
}

fn strip_task_marker(item: &str) -> &str {
    item.strip_prefix("[ ]")
        .or_else(|| item.strip_prefix("[x]"))
        .or_else(|| item.strip_prefix("[X]"))
        .unwrap_or(item)
        .trim()
}

/// 要点限长:超出按字符截断并补省略号(字符数,非字节数)。
fn clamp_point(point: &str) -> String {
    if point.chars().count() <= POINT_LIMIT_CHARS {
        point.to_owned()
    } else {
        let head: String = point.chars().take(POINT_LIMIT_CHARS - 1).collect();
        format!("{head}…")
    }
}

/// 按 diff 关键词选 conventional 前缀。关键词全部取自 diff 的文件路径与
/// 元数据行,不会撞上 prompt 的指令文本(见 commit.rs INSTRUCTIONS 的措辞
/// 约束);`new file mode` 排在路径规则之后——新增 README 应是 docs 而非 feat。
fn commit_prefix(prompt: &str) -> &'static str {
    if prompt.contains("README") || prompt.contains(".md") || prompt.contains("docs/") {
        "docs"
    } else if prompt.contains("Cargo.toml") || prompt.contains("Cargo.lock") {
        "chore"
    } else if prompt.contains("new file mode") {
        "feat"
    } else {
        "chore"
    }
}

/// 按 diff 元数据行选动作:新增 / 清理 / 更新。
fn commit_action(prompt: &str) -> &'static str {
    if prompt.contains("new file mode") {
        "新增"
    } else if prompt.contains("deleted file mode") {
        "清理"
    } else {
        "更新"
    }
}

/// 取 diff 第一处改动的文件名(`+++ b/<path>` 行的 basename);删除侧的
/// `+++ /dev/null` 天然不匹配该前缀,取不到时回退到通用描述。
fn changed_file(prompt: &str) -> Option<&str> {
    let line = prompt.lines().find(|line| line.starts_with("+++ b/"))?;
    let path = &line["+++ b/".len()..];
    let name = path.rsplit('/').next()?;
    (!name.is_empty()).then_some(name)
}

/// 前缀 + 动作 + (文件名 | 通用描述),拼成单行 subject。
fn commit_subject_for(prompt: &str) -> String {
    match changed_file(prompt) {
        Some(file) => format!(
            "{}: {}{}",
            commit_prefix(prompt),
            commit_action(prompt),
            file
        ),
        None => format!(
            "{}: {}项目文件",
            commit_prefix(prompt),
            commit_action(prompt)
        ),
    }
}

impl AiProvider for MockProvider {
    fn stream_complete(&self, prompt: &str, tx: Sender<Chunk>) -> JoinHandle<()> {
        let script = script_for(prompt);
        let interval = self.interval;
        thread::Builder::new()
            .name("latermd-ai-mock".into())
            .spawn(move || {
                for piece in script {
                    thread::sleep(interval);
                    if tx
                        .send(Chunk {
                            delta: piece,
                            done: false,
                        })
                        .is_err()
                    {
                        return; // 接收端取消,静默收尾
                    }
                }
                let _ = tx.send(Chunk {
                    delta: String::new(),
                    done: true,
                });
            })
            .expect("spawn latermd-ai mock worker")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Instant;

    fn script_text(prompt: &str) -> String {
        script_for(prompt).concat()
    }

    #[test]
    fn mock_stream_concatenates_to_full_text() {
        let provider = MockProvider::new();
        let (tx, rx) = mpsc::channel();
        let start = Instant::now();
        let handle = provider.stream_complete("帮我续写这段设计文档", tx);

        let mut got = String::new();
        let mut count = 0usize;
        for chunk in rx {
            if chunk.done {
                assert!(chunk.delta.is_empty(), "成功结束块不得携带正文");
                break;
            }
            let len = chunk.delta.chars().count();
            assert!((MIN_CHUNK..=MAX_CHUNK).contains(&len), "块长 {len} 越界");
            got.push_str(&chunk.delta);
            count += 1;
        }
        handle.join().unwrap();

        assert_eq!(got, script_text("帮我续写这段设计文档"));
        // 块数 30-50 × 100ms ≈ 3-5 秒(ask 的总时长约束)
        assert!((30..=50).contains(&count), "块数 {count} 越界");
        let elapsed = start.elapsed();
        assert!(
            elapsed.as_secs_f32() >= 2.8,
            "总时长 {elapsed:?} 低于 3 秒档"
        );
        assert!(elapsed < Duration::from_secs(20), "总时长 {elapsed:?} 异常");
    }

    #[test]
    fn mock_script_selection_follows_prompt() {
        // 代码关键词命中代码密集脚本
        assert_eq!(script_text("帮我写段代码"), script_text("实现一个 thing"));
        assert_eq!(script_text("a CODE demo"), script_text("实现一个 thing"));
        // 普通续写走默认脚本,且两套脚本确实不同
        assert_ne!(script_text("总结一下这一节"), script_text("帮我写段代码"));
    }

    #[test]
    fn chunk_text_is_lossless_and_respects_bounds() {
        let cases = [
            "短文本".to_owned(),
            "x".repeat(105),
            "中".repeat(41),
            "y".repeat(20),
            format!(
                "{}\n{}\n```rust\nlet a = 1;\n```\n",
                "段".repeat(25),
                "落".repeat(30)
            ),
        ];
        for case in &cases {
            let chunks = chunk_text(case, CHUNK_TARGET);
            assert_eq!(chunks.concat(), case.as_str(), "分块必须无损");
            if chunks.len() > 1 {
                for c in &chunks {
                    let len = c.chars().count();
                    assert!((MIN_CHUNK..=MAX_CHUNK).contains(&len), "块长 {len} 越界");
                }
            }
        }
    }

    /// commit 场景:用真实 prompt 模板喂 mock(顺带验证指令措辞不与关键词
    /// 规则撞车),按 diff 形态产出对应前缀/动作/文件名的单行 subject。
    #[test]
    fn mock_commit_subject_follows_diff_keywords() {
        let subject_for = |diff: &str| {
            MockProvider::new().mock_commit_subject(&crate::commit::commit_message_prompt(diff))
        };

        // 新增 README:路径规则优先于 new file → docs + 新增 + 文件名
        let new_readme = "diff --git a/README.md b/README.md\nnew file mode 100644\n\
                          --- /dev/null\n+++ b/README.md\n@@ -0,0 +1 @@\n+# 说明\n";
        assert_eq!(subject_for(new_readme), "docs: 新增README.md");

        // 新增源码文件:feat
        let new_src = "diff --git a/src/editor.rs b/src/editor.rs\nnew file mode 100644\n\
                       --- /dev/null\n+++ b/src/editor.rs\n@@ -0,0 +1 @@\n+fn main() {}\n";
        assert_eq!(subject_for(new_src), "feat: 新增editor.rs");

        // 修改既有源码:chore + 更新
        let edit = "diff --git a/src/main.rs b/src/main.rs\n--- a/src/main.rs\n\
                    +++ b/src/main.rs\n@@ -1 +1 @@\n-fn old() {}\n+fn new() {}\n";
        assert_eq!(subject_for(edit), "chore: 更新main.rs");

        // 构建清单:路径规则命中 chore(与默认同前缀,但显式覆盖「非代码非文档」类)
        let cargo = "diff --git a/Cargo.toml b/Cargo.toml\n--- a/Cargo.toml\n\
                     +++ b/Cargo.toml\n@@ -1 +1 @@\n-ropey = \"1.6\"\n+ropey = \"1.7\"\n";
        assert_eq!(subject_for(cargo), "chore: 更新Cargo.toml");

        // 删除文件:+++ 侧是 /dev/null,取不到文件名 → 通用描述
        let deleted = "diff --git a/src/old.rs b/src/old.rs\ndeleted file mode 100644\n\
                       --- a/src/old.rs\n+++ /dev/null\n@@ -1 +0,0 @@\n-fn gone() {}\n";
        assert_eq!(subject_for(deleted), "chore: 清理项目文件");

        // 空 diff(防御):通用兜底
        assert_eq!(subject_for(""), "chore: 更新项目文件");
    }

    /// 产出形态约束:单行、无反引号、前缀在约定集合内(所有分支都过一遍)。
    #[test]
    fn mock_commit_subject_is_single_line_with_conventional_prefix() {
        let diffs = [
            "new file mode 100644\n+++ b/README.md\n",
            "+++ b/src/a.rs\n",
            "deleted file mode 100644\n+++ /dev/null\n",
            "",
        ];
        for diff in diffs {
            let subject = MockProvider::new().mock_commit_subject(diff);
            assert!(!subject.contains('\n'), "{subject}");
            assert!(!subject.contains('`'), "{subject}");
            assert!(
                ["feat: ", "fix: ", "docs: ", "chore: "]
                    .iter()
                    .any(|prefix| subject.starts_with(prefix)),
                "{subject}"
            );
        }
    }

    /// 用真实 prompt 模板喂 mock(顺带验证指令行不被抽成要点),标题与
    /// 列表关键词按文档顺序成为要点,任务标记剥掉,产出是引用块列表。
    #[test]
    fn mock_summary_follows_heading_and_list_keywords() {
        let doc = "# 缓存设计\n\n定时刷新有击穿风险。\n\n## 失效策略\n\n\
                   - 写入时失效\n- 读写穿透会拖慢冷启动\n1. 事务提交后触发\n- [ ] 补充压测数据\n";
        let summary = MockProvider::new().mock_summary(&crate::summary::summary_prompt(doc));
        let lines: Vec<&str> = summary.lines().collect();
        assert_eq!(
            lines,
            vec![
                "> - 缓存设计",
                "> - 失效策略",
                "> - 写入时失效",
                "> - 读写穿透会拖慢冷启动",
                "> - 事务提交后触发",
            ],
            "{summary}"
        );
        assert!(summary.ends_with('\n'), "行尾换行,追加后成规范段落");
        // 第 4 条列表(任务项)在 5 条上限外被丢弃
        assert!(!summary.contains("压测"), "{summary}");
        // 指令头不是要点候选
        assert!(!summary.contains("摘要助手"), "{summary}");
    }

    /// 候选不足 3 条时用段落行补足;引用/表格/代码块内的行永远不抽;
    /// 空到没有可抽行时兜底单条,绝不返回空串。
    #[test]
    fn mock_summary_pads_with_paragraphs_and_skips_quote_table_code() {
        // 1 个标题 + 1 个列表 + 1 段正文 → 补足到 3 条
        let doc = "# 标题\n\n- 要点\n\n正文段落一行。\n";
        let summary = MockProvider::new().mock_summary(&crate::summary::summary_prompt(doc));
        assert_eq!(
            summary.lines().collect::<Vec<_>>(),
            vec!["> - 标题", "> - 要点", "> - 正文段落一行。"],
            "{summary}"
        );

        // 引用块/表格/围栏代码里的行不是候选:只有两个无序列表可用,
        // 段落补足不引用它们(此处无普通段落,停留在 2 条 —— 不硬凑)
        let doc = "> 引用里的行不算\n\n| 表 | 格 |\n|---|---|\n\n```rust\n// 注释也不算\n```\n\n- 甲\n- 乙\n";
        let summary = MockProvider::new().mock_summary(&crate::summary::summary_prompt(doc));
        assert_eq!(summary.lines().count(), 2, "{summary}");
        assert!(!summary.contains("引用里"), "{summary}");
        assert!(!summary.contains("表"), "{summary}");
        assert!(!summary.contains("注释"), "{summary}");

        // 空文档(调用方拦截,这里是防御):兜底单条
        let summary = MockProvider::new().mock_summary(&crate::summary::summary_prompt(""));
        assert_eq!(summary.lines().count(), 1, "{summary}");
        assert!(summary.starts_with("> - "), "{summary}");
    }

    /// 单条要点限长:超出 40 字符截断补省略号(按字符,CJK 不拆半)。
    #[test]
    fn mock_summary_clamps_long_points_by_chars() {
        let doc = format!("# {}\n", "长".repeat(60));
        let summary = MockProvider::new().mock_summary(&crate::summary::summary_prompt(&doc));
        let line = summary.lines().next().unwrap();
        assert!(line.ends_with('…'), "{line}");
        assert_eq!(
            line.chars().count(),
            "> - ".chars().count() + POINT_LIMIT_CHARS
        );
    }

    /// 摘要走流式通道:摘要 prompt 喂 `stream_complete`,块拼起来恰是
    /// `mock_summary` 的产物,块长落在 20-80 区间(60 目标 + 尾块并入)。
    #[test]
    fn summary_prompt_streams_as_chunked_mock_summary() {
        let doc =
            "# 缓存与一致性设计总览\n\n本节讨论缓存失效的三种策略与最终选择。\n\n## 写入时失效\n\n\
                   - 与业务事务天然对齐,回滚时能一并撤销\n- 定时刷新在高峰期会造成缓存击穿\n\
                   - 读写穿透把一致性责任压在读路径上\n";
        let prompt = crate::summary::summary_prompt(doc);
        let expected = MockProvider::new().mock_summary(&prompt);

        let provider = MockProvider::with_interval(Duration::ZERO);
        let (tx, rx) = mpsc::channel();
        provider.stream_complete(&prompt, tx);

        let mut got = String::new();
        for chunk in rx {
            if chunk.done {
                assert!(chunk.delta.is_empty(), "成功结束块不得携带正文");
                break;
            }
            let len = chunk.delta.chars().count();
            assert!((MIN_CHUNK..=MAX_CHUNK).contains(&len), "块长 {len} 越界");
            got.push_str(&chunk.delta);
        }
        assert_eq!(got, expected, "流式拼接与同步合成一致");
        assert!(got.chars().count() > SUMMARY_CHUNK_TARGET, "确实分了多块");
    }
}
