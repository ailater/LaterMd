//! 仓库全文搜索核心(P1 侧边栏搜索,docs/roadmap.md 阶段 3):后台线程
//! 遍历 + 逐文件逐行正则匹配,命中经有界 channel 流式回传。
//!
//! - 行迭代用 `grep_searcher::LineIter`(与 ripgrep 同一套行语义:产出行
//!   含终止符、尾行可不带换行)。不走 `Searcher::search_*` 的原因:那组
//!   API 全部要求 `grep_matcher::Matcher` 实参,`regex::Regex` 并未实现该
//!   trait,直连不成立;补引 `grep-regex` 桥接则是又一个清单外依赖,收益
//!   只有 trait 适配,不值(decisions-pending #7)。
//! - 取消是代际号语义:`spawn`/`cancel` 都推进全局代际,旧线程在检查点
//!   自行退出,迟到的旧代事件由接收端按代际丢弃;UI 侧 `try_recv` 永不
//!   阻塞,也不会 join 后台线程。
//! - 去抖不属于本层:输入节奏由 UI 决定,服务只提供发起/接收/取消三个
//!   原语。
//!
//! 根目录不存在时遍历为空、直接收到 `Done`(零命中),不视为错误——根
//! 目录的存在性由文件树侧保证。

#![allow(dead_code)]
// P1 搜索面板接驳前,本模块在 bin crate 内暂无调用方,先以「核心 + 测试」
// 形态落地;UI 接入后删除本属性。

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::Arc;
use std::thread;

use grep_searcher::LineIter;
use ignore::WalkBuilder;
// bytes 变体:`LineIter` 产出 `&[u8]`,str 版 `Regex` 不接受字节入参;
// 非法 UTF-8 的行由 trim_line_terminator 的 lossy 兜底,匹配层不必是 str
use regex::bytes::{Regex, RegexBuilder};

use crate::file::MARKDOWN_EXTENSIONS;

/// 结果 channel 容量:满时后台线程阻塞在 `send`(背压),防海量命中把
/// 内存吃穿;UI 侧只 `try_recv`,不会被背压波及。
const CHANNEL_CAPACITY: usize = 256;

/// 单文件读入上限:行迭代需要完整字节切片,病态大文件(哪怕是 `.md`)
/// 整体跳过,不读进内存。
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

/// 一次搜索请求。
#[derive(Debug, Clone)]
pub struct SearchQuery {
    pub root: PathBuf,
    /// 正则模式(用户输入按正则解释;大小写开关独立于模式本身)。
    pub pattern: String,
    pub case_insensitive: bool,
}

/// 一条命中:文件 + 1 起行号 + 剥掉行终止符的行文本。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    pub path: PathBuf,
    pub line_no: usize,
    pub line_text: String,
}

/// `try_recv` 产出的事件:`Hit` 持续流出;`Done` 表示当前代搜索自然结束
/// (被取代或取消的搜索不产生 `Done`)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchEvent {
    Hit(SearchResult),
    Done,
}

/// 发起失败的情形。
#[derive(Debug)]
pub enum SearchError {
    /// 空模式,或正则编译失败(输入到一半是常态,消息可直接展示)。
    InvalidPattern(String),
    /// 后台线程创建失败(系统级,罕见)。
    Spawn(std::io::Error),
}

impl fmt::Display for SearchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SearchError::InvalidPattern(msg) => write!(f, "搜索正则无效: {msg}"),
            SearchError::Spawn(source) => write!(f, "搜索线程启动失败: {source}"),
        }
    }
}

/// 线程间共享的代际号:最新一代是唯一值得产出结果的搜索。
#[derive(Debug, Default)]
struct Shared {
    generation: AtomicU64,
}

/// channel 内部事件:携带代际号,接收端据此丢弃旧代迟到结果。
#[derive(Debug)]
enum Event {
    Hit { gen: u64, result: SearchResult },
    Done { gen: u64 },
}

/// 搜索服务。同一时刻只有一个「当前」搜索:再次 `spawn` 或 `cancel` 都会
/// 让前一代即刻作废。调用方(UI logic)单线程持有,本类型不为并发调用
/// 设计。
pub struct SearchService {
    shared: Arc<Shared>,
    tx: SyncSender<Event>,
    rx: Receiver<Event>,
}

impl SearchService {
    pub fn new() -> Self {
        let (tx, rx) = sync_channel(CHANNEL_CAPACITY);
        Self {
            shared: Arc::new(Shared::default()),
            tx,
            rx,
        }
    }

    /// 发起一次搜索;旧搜索即刻作废。正则在发起线程编译,非法模式同步
    /// 报错、不进后台线程。
    pub fn spawn(&mut self, query: SearchQuery) -> Result<(), SearchError> {
        if query.pattern.is_empty() {
            return Err(SearchError::InvalidPattern("搜索词为空".into()));
        }
        let regex = RegexBuilder::new(&query.pattern)
            .case_insensitive(query.case_insensitive)
            .build()
            .map_err(|error| SearchError::InvalidPattern(error.to_string()))?;
        // fetch_add 返回旧值,新代 = 旧值 + 1;旧线程下个检查点即退出
        let gen = self.shared.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let shared = Arc::clone(&self.shared);
        let tx = self.tx.clone();
        thread::Builder::new()
            .name("latermd-search".into())
            .spawn(move || run(query, regex, gen, shared, tx))
            .map_err(SearchError::Spawn)?;
        Ok(())
    }

    /// 非阻塞接收一条当前代事件;channel 空返回 `None`。旧代迟到事件在
    /// 此被静默丢弃。
    pub fn try_recv(&mut self) -> Option<SearchEvent> {
        let current = self.shared.generation.load(Ordering::SeqCst);
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::Hit { gen, result } if gen == current => {
                    return Some(SearchEvent::Hit(result));
                }
                Event::Done { gen } if gen == current => return Some(SearchEvent::Done),
                // 旧代迟到事件:丢弃,继续取
                _ => continue,
            }
        }
        None
    }

    /// 取消当前搜索:推进代际作废全部在途结果,并清空积压事件——放行可
    /// 能阻塞在 `send` 上的旧线程(它发出这条后在下个检查点退出,不会
    /// 永久滞留)。
    pub fn cancel(&mut self) {
        self.shared.generation.fetch_add(1, Ordering::SeqCst);
        while self.rx.try_recv().is_ok() {}
    }
}

impl Default for SearchService {
    fn default() -> Self {
        Self::new()
    }
}

/// 后台线程主体:遍历 → 逐文件读入 → 逐行匹配 → 发送命中。取消检查点
/// 在「每进一个条目」与「每发一条命中」;单文件的行循环里不查——内存
/// 中扫一遍是毫秒级,不值得为此加原子读。
fn run(query: SearchQuery, regex: Regex, gen: u64, shared: Arc<Shared>, tx: SyncSender<Event>) {
    let superseded = || shared.generation.load(Ordering::SeqCst) != gen;
    // require_git(false):无 .git 的普通目录也吃自己写的 .gitignore,与
    // 文件树(`crate::filetree`)同一直觉;遍历错误(权限、条目消失)跳过
    for entry in WalkBuilder::new(&query.root)
        .require_git(false)
        .build()
        .flatten()
    {
        if superseded() {
            return;
        }
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let path = entry.path();
        if !is_markdown(path) {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.len() > MAX_FILE_BYTES {
            continue;
        }
        // 读失败(权限、竞争删除)跳过该文件:搜索是导航手段,不因个别
        // 文件不可读而整体失败
        let Ok(haystack) = std::fs::read(path) else {
            continue;
        };
        for (index, line) in LineIter::new(b'\n', &haystack).enumerate() {
            if !regex.is_match(line) {
                continue;
            }
            if superseded() {
                return;
            }
            let result = SearchResult {
                path: path.to_path_buf(),
                line_no: index + 1,
                line_text: trim_line_terminator(line),
            };
            if tx.send(Event::Hit { gen, result }).is_err() {
                return; // 接收端已随服务销毁,退出
            }
        }
    }
    let _ = tx.send(Event::Done { gen });
}

/// 扩展名是否 Markdown(与文件树同一张清单,见 `crate::file::MARKDOWN_EXTENSIONS`)。
fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            MARKDOWN_EXTENSIONS
                .iter()
                .any(|known| ext.eq_ignore_ascii_case(known))
        })
}

/// `LineIter` 产出的行自带终止符(`\n`,CRLF 文件还带 `\r`);入库前剥掉,
/// 展示层拿到的就是纯行文本。非 UTF-8 内容替换为 U+FFFD,不因个别坏档
/// 让整次搜索失败。
fn trim_line_terminator(line: &[u8]) -> String {
    String::from_utf8_lossy(line)
        .trim_end_matches(['\r', '\n'])
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// 进程内唯一的临时目录;测试自删。
    fn temp_vault(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("latermd-search-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// 造样本仓库:3 个有效 md + `.gitignore` 排除项 + 非 md 二进制。
    /// - `a.md`:`LaterMD`(精确大小写)+ 中文「架构」
    /// - `notes/b.markdown`(嵌套子目录):小写 `latermd`,供大小写不敏感用
    /// - `c.md`:无关键词(负样本)
    /// - `ignored.md` / `drafts/hidden.md`:被 `.gitignore` 排除
    /// - `blob.bin`:含关键词的非 md 二进制(扩展名过滤的对象)
    fn sample_vault(name: &str) -> PathBuf {
        let root = temp_vault(name);
        std::fs::write(
            root.join("a.md"),
            "LaterMD 是 Markdown 工作台\n普通一行\n架构决策记录\n",
        )
        .unwrap();
        std::fs::create_dir(root.join("notes")).unwrap();
        std::fs::write(root.join("notes/b.markdown"), "latermd 不分大小写\n").unwrap();
        std::fs::write(root.join("c.md"), "这里没有关键词\n").unwrap();
        std::fs::write(root.join(".gitignore"), "ignored.md\ndrafts/\n").unwrap();
        std::fs::write(root.join("ignored.md"), "latermd 不该被搜到\n").unwrap();
        std::fs::create_dir(root.join("drafts")).unwrap();
        std::fs::write(root.join("drafts/hidden.md"), "latermd 草稿也不搜\n").unwrap();
        std::fs::write(root.join("blob.bin"), b"latermd\x00\x01binary").unwrap();
        root
    }

    fn query(root: &Path, pattern: &str, case_insensitive: bool) -> SearchQuery {
        SearchQuery {
            root: root.to_path_buf(),
            pattern: pattern.into(),
            case_insensitive,
        }
    }

    /// 用公开原语收全当前代结果:收到 `Done` 或 10s 超时为止。
    /// 返回(命中列表, 是否正常收到 Done)。
    fn drain(service: &mut SearchService) -> (Vec<SearchResult>, bool) {
        let mut hits = Vec::new();
        let mut done = false;
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done && Instant::now() < deadline {
            match service.try_recv() {
                Some(SearchEvent::Hit(hit)) => hits.push(hit),
                Some(SearchEvent::Done) => done = true,
                None => std::thread::sleep(Duration::from_millis(2)),
            }
        }
        (hits, done)
    }

    /// 拍平成可比较集合(路径、行号、行文本)。
    fn hit_triples(hits: &[SearchResult]) -> Vec<(PathBuf, usize, String)> {
        hits.iter()
            .map(|hit| (hit.path.clone(), hit.line_no, hit.line_text.clone()))
            .collect()
    }

    /// 大小写敏感:只命中精确写法;行文本不含换行符;`.gitignore` 排除项
    /// 与非 md 二进制(`blob.bin` 同含关键词)不出现——集合精确相等即同时
    /// 覆盖这两点。
    #[test]
    fn case_sensitive_hits_exact_only() {
        let root = sample_vault("case-sensitive");
        let mut service = SearchService::new();
        service.spawn(query(&root, "LaterMD", false)).unwrap();
        let (hits, done) = drain(&mut service);

        assert!(done, "搜索应自然结束并发出 Done");
        assert_eq!(
            hit_triples(&hits),
            vec![(root.join("a.md"), 1, "LaterMD 是 Markdown 工作台".into())]
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// 大小写不敏感:嵌套子目录里的小写变体一并命中(按路径排序稳定断言)。
    #[test]
    fn case_insensitive_hits_nested_variant() {
        let root = sample_vault("case-insensitive");
        let mut service = SearchService::new();
        service.spawn(query(&root, "LaterMD", true)).unwrap();
        let (hits, done) = drain(&mut service);

        assert!(done);
        let mut triples = hit_triples(&hits);
        triples.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            triples,
            vec![
                (root.join("a.md"), 1, "LaterMD 是 Markdown 工作台".into()),
                (
                    root.join("notes/b.markdown"),
                    1,
                    "latermd 不分大小写".into()
                ),
            ]
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// 中文关键词走同一条正则路径(regex 的 Unicode 支持),行号正确(第 3 行)。
    #[test]
    fn chinese_pattern_hits_third_line() {
        let root = sample_vault("chinese");
        let mut service = SearchService::new();
        service.spawn(query(&root, "架构", false)).unwrap();
        let (hits, done) = drain(&mut service);

        assert!(done);
        assert_eq!(
            hit_triples(&hits),
            vec![(root.join("a.md"), 3, "架构决策记录".into())]
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// 取消后不再产出任何事件(含 Done)。sleep 覆盖两种时序:线程已塞入
    /// 结果(被 cancel 的 drain 丢弃)或尚未启动(下个检查点即退出)。
    #[test]
    fn cancel_stops_production() {
        let root = sample_vault("cancel");
        let mut service = SearchService::new();
        service.spawn(query(&root, "latermd", true)).unwrap();
        std::thread::sleep(Duration::from_millis(50));
        service.cancel();

        let deadline = Instant::now() + Duration::from_millis(200);
        while Instant::now() < deadline {
            assert_eq!(
                service.try_recv(),
                None,
                "取消后不应再有任何事件(含旧代迟到的 Done)"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// 新搜索发起后,旧搜索即使已把结果塞进 channel,也只允许新代命中
    /// 与新代 Done 流出。
    #[test]
    fn superseded_search_yields_only_new_generation() {
        let root = sample_vault("supersede");
        let mut service = SearchService::new();
        // 第一代:唯一命中是 a.md 第 2 行「普通一行」
        service.spawn(query(&root, "普通", false)).unwrap();
        std::thread::sleep(Duration::from_millis(50)); // 给第一代塞结果的机会
                                                       // 第二代取代之
        service.spawn(query(&root, "latermd", true)).unwrap();
        let (hits, done) = drain(&mut service);

        assert!(done);
        let mut triples = hit_triples(&hits);
        triples.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            triples,
            vec![
                (root.join("a.md"), 1, "LaterMD 是 Markdown 工作台".into()),
                (
                    root.join("notes/b.markdown"),
                    1,
                    "latermd 不分大小写".into()
                ),
            ],
            "第一代命中(a.md:2「普通一行」)不允许混入"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// 空模式与未闭合括号在发起线程同步报错,不产生任何事件。
    #[test]
    fn invalid_pattern_is_rejected_upfront() {
        let root = sample_vault("invalid");
        let mut service = SearchService::new();
        assert!(matches!(
            service.spawn(query(&root, "(", false)),
            Err(SearchError::InvalidPattern(_))
        ));
        assert!(matches!(
            service.spawn(query(&root, "", false)),
            Err(SearchError::InvalidPattern(_))
        ));
        assert_eq!(service.try_recv(), None, "发起失败不应有事件");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// 超过单文件上限的 md 整体跳过(行迭代要整读进内存),而非部分命中。
    #[test]
    fn oversized_markdown_is_skipped() {
        let root = temp_vault("oversize");
        let mut bytes = b"latermd marker\n".to_vec();
        bytes.resize(MAX_FILE_BYTES as usize + 1, b'.');
        std::fs::write(root.join("big.md"), bytes).unwrap();

        let mut service = SearchService::new();
        service.spawn(query(&root, "latermd", false)).unwrap();
        let (hits, done) = drain(&mut service);
        assert!(done);
        assert!(hits.is_empty(), "超限文件跳过,首行关键词也不产出");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// CRLF 文件:行号语义与 LF 一致,行文本剥掉 `\r\n`。
    #[test]
    fn crlf_lines_are_trimmed() {
        let root = temp_vault("crlf");
        std::fs::write(root.join("win.md"), b"first\r\nlatermd here\r\n").unwrap();

        let mut service = SearchService::new();
        service.spawn(query(&root, "latermd", false)).unwrap();
        let (hits, done) = drain(&mut service);
        assert!(done);
        assert_eq!(
            hit_triples(&hits),
            vec![(root.join("win.md"), 2, "latermd here".into())]
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
