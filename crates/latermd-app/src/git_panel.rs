//! Git 面板状态(P2 版本层,docs/roadmap.md 阶段 4):[`latermd_git`] 的
//! UI 接驳层,归约持有、渲染只读(adr-005 的 logic/ui 二分)。
//!
//! - **刷新时机**:节流轮询,而非每帧 `git status`。间隔 [`REFRESH_INTERVAL`]
//!   (3s),由归约侧(`ui::layout::reduce`)到点触发;换根、切到 Git 页与
//!   回滚完成走同一入口立即刷新。同步执行——git status/log 是毫秒级本地
//!   读,与 `git_diff.rs` 同一取舍,不值得上后台线程(大仓库冷缓存若实测
//!   掉帧,再挪线程,接口不变)。
//! - **降级**:文件树根不是 git 仓库时 `error` 置文案、角标与列表清空,
//!   Git 页显示提示而非报错;降级后**停止轮询**(重探由换根/切 Git 页触发,
//!   egui 得以收敛到深度空闲),无根时静默清空、由渲染层给引导。
//! - **回滚**:[`latermd_git::checkout_file`] 是 latermd-git 唯一写操作,
//!   本状态只把它暴露在确认模态之后(`confirm_checkout` 由 UI 点按钮置入,
//!   用户显式确认才执行)。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use latermd_git::{CommitInfo, FileStatus, StatusKind};

/// Git 状态自动刷新间隔。3s 在「角标足够新」与「零感开销」之间取中:
/// 每次刷新是两次毫秒级本地读(status + log),空闲时每 3s 唤醒一帧。
pub const REFRESH_INTERVAL: Duration = Duration::from_secs(3);

/// Git 面板状态:一次刷新的快照(status/log/diff 缓存)+ 确认式回滚。
///
/// 全部字段由归约维护,渲染层只读。`repo_root` 是 [`latermd_git::discover`]
/// 的结果——文件树根可能是仓库子目录,状态路径统一记「相对仓库根」,
/// 角标按需拼成绝对路径与文件树条目匹配。
#[derive(Debug)]
pub struct GitPanelState {
    /// 仓库根(discover 结果);`None` = 无根 / 非 git 仓库 / 尚未刷过。
    pub repo_root: Option<PathBuf>,
    /// 降级文案(非 git 仓库等);`Some` = Git 页显示它,列表与历史为空。
    pub error: Option<String>,
    /// 改动列表(`latermd_git::status` 原样:相对仓库根、按路径排序)。
    pub entries: Vec<FileStatus>,
    /// 状态角标:绝对路径 → 状态码,文件树 Files 页与 Git 页共用。
    pub badges: HashMap<PathBuf, StatusKind>,
    /// 近期提交(`latermd_git::log` 原样,新在前;log 失败静默为空——
    /// status 成功即仓库可用,历史失败无独立展示价值)。
    pub commits: Vec<CommitInfo>,
    /// 刷新时刻的 Unix 秒,相对时间渲染的基准(分钟级精度,无需逐帧取)。
    pub fetched_at: i64,
    /// 选中的文件(相对仓库根);`None` = 未选中,diff 区不显示。
    pub selected: Option<String>,
    /// 选中文件的 diff 文本(错误文案也进这里,只读区直接可见);
    /// 空串 = 无文本改动。
    pub diff: String,
    /// 待确认回滚的文件(相对仓库根);`Some` = 确认模态可见。
    pub confirm_checkout: Option<String>,
    /// 下次自动刷新时刻;到点由归约侧轮询触发(见模块文档)。
    pub refresh_due: Instant,
}

impl Default for GitPanelState {
    /// 空快照;`refresh_due` 取当前时刻——首轮轮询即刻到点,有根就先刷一次。
    fn default() -> Self {
        Self {
            repo_root: None,
            error: None,
            entries: Vec::new(),
            badges: HashMap::new(),
            commits: Vec::new(),
            fetched_at: 0,
            selected: None,
            diff: String::new(),
            confirm_checkout: None,
            refresh_due: Instant::now(),
        }
    }
}

impl GitPanelState {
    /// 立即刷新(轮询到点、换根、切到 Git 页、回滚完成共用):discover →
    /// status → log,任一步失败即降级并把旧快照作废;`tree_root` 为
    /// `None` 时静默清空。选中项若已离开改动列表(外部提交/回滚)一并失效。
    pub fn refresh(&mut self, tree_root: Option<&Path>) {
        self.refresh_due = Instant::now() + REFRESH_INTERVAL;
        self.fetched_at = unix_now();
        let Some(root) = tree_root else {
            self.degrade(None);
            return;
        };
        let repo_root = match latermd_git::discover(root) {
            Ok(root) => root,
            Err(error) => {
                self.degrade(Some(error));
                return;
            }
        };
        let entries = match latermd_git::status(&repo_root) {
            Ok(entries) => entries,
            Err(error) => {
                self.degrade(Some(error));
                return;
            }
        };
        self.repo_root = Some(repo_root.clone());
        self.error = None;
        self.badges = entries
            .iter()
            .map(|entry| (repo_root.join(&entry.path), entry.code))
            .collect();
        if let Some(selected) = &self.selected {
            if entries.iter().any(|entry| &entry.path == selected) {
                self.reload_diff();
            } else {
                self.selected = None;
                self.diff.clear();
            }
        }
        self.entries = entries;
        self.commits =
            latermd_git::log(&repo_root, latermd_git::DEFAULT_LOG_LIMIT).unwrap_or_default();
    }

    /// 选中改动列表里的一个文件(`Message::GitFileSelected` 的归约):
    /// 选中并读 diff。不在当前列表里的路径(过期消息)忽略。
    pub fn select(&mut self, path: &str) {
        if !self.entries.iter().any(|entry| entry.path == path) {
            return;
        }
        self.selected = Some(path.to_owned());
        self.reload_diff();
    }

    /// 点「回滚此文件」(`Message::GitCheckoutRequested` 的归约):只置确认
    /// 模态。真正的写操作在用户显式确认之后([`Self::confirm_checkout`])。
    pub fn request_checkout(&mut self, path: String) {
        if self.entries.iter().any(|entry| entry.path == path) {
            self.confirm_checkout = Some(path);
        }
    }

    /// 确认模态取消(`Message::GitCheckoutCancelled` 的归约)。
    pub fn cancel_checkout(&mut self) {
        self.confirm_checkout = None;
    }

    /// 确认模态的「回滚」(`Message::GitCheckoutConfirmed` 的归约):执行
    /// latermd-git 唯一的写操作并立即刷新状态;模态总是关闭(成功失败都
    /// 不再挂着)。失败返回文案给调用方进提示行,刷新照做保真。
    pub fn confirm_checkout(&mut self, tree_root: Option<&Path>) -> Option<String> {
        let path = self.confirm_checkout.take()?;
        let root = self.repo_root.clone()?;
        let error = latermd_git::checkout_file(&root, &path).err();
        self.refresh(tree_root);
        error
    }

    /// 文件树角标查询:绝对路径在改动列表中的状态码。
    pub fn badge_for(&self, file: &Path) -> Option<StatusKind> {
        self.badges.get(file).copied()
    }

    /// 自动刷新是否到点(归约侧每帧问一次)。
    pub fn due(&self) -> bool {
        self.refresh_due <= Instant::now()
    }

    /// 距下次刷新的时长(egui `request_repaint_after` 的参数:空闲不来帧,
    /// 轮询依赖显式要帧)。
    pub fn until_refresh(&self) -> Duration {
        self.refresh_due.saturating_duration_since(Instant::now())
    }

    /// 作废旧快照;`error` 为 `None` 时是无根的静默清空。
    fn degrade(&mut self, error: Option<String>) {
        self.error = error;
        self.repo_root = None;
        self.entries.clear();
        self.badges.clear();
        self.commits.clear();
        self.selected = None;
        self.diff.clear();
        self.confirm_checkout = None;
    }

    /// 按当前选中项重读 diff。
    fn reload_diff(&mut self) {
        self.diff = match (&self.repo_root, &self.selected) {
            (Some(root), Some(path)) => match latermd_git::diff_file(root, path) {
                Ok(text) => text,
                Err(error) => error, // 错误文案进只读区,用户可见
            },
            _ => String::new(),
        };
    }
}

/// 当前 Unix 秒;系统时钟早于 epoch(不可能的病态)按 0。
fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    /// 建一次性临时仓库并提交一个文件(每次全新目录,测试间无共享状态)。
    /// fixture 用系统 git CLI 装配(与 latermd-git 同模式),被测逻辑走
    /// GitPanelState 自身。
    fn temp_repo(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("latermd-gitpanel-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q"]);
        std::fs::write(dir.join("a.md"), "HEAD 版本\n").unwrap();
        git(&dir, &["add", "."]);
        git(&dir, &["commit", "-q", "-m", "init"]);
        dir
    }

    fn git(dir: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(["-c", "user.name=LaterMD", "-c", "user.email=latermd@test"])
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?} 失败: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// 刷新填充:改动列表、绝对路径角标与历史;子目录作根同样命中(discover)。
    #[test]
    fn refresh_populates_entries_badges_and_log() {
        let dir = temp_repo("refresh");
        std::fs::write(dir.join("a.md"), "改\n").unwrap(); // M
        std::fs::write(dir.join("new.md"), "?\n").unwrap(); // ?
        std::fs::create_dir_all(dir.join("docs")).unwrap();

        let mut git = GitPanelState::default();
        git.refresh(Some(&dir));
        assert_eq!(git.error, None);
        assert_eq!(git.repo_root.as_deref(), Some(dir.as_path()));
        let paths: Vec<&str> = git.entries.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, vec!["a.md", "new.md"], "按路径排序");
        assert_eq!(git.badge_for(&dir.join("a.md")), Some(StatusKind::Modified));
        assert_eq!(
            git.badge_for(&dir.join("new.md")),
            Some(StatusKind::Untracked)
        );
        assert_eq!(git.badge_for(&dir.join("docs")), None, "目录无角标");
        assert_eq!(git.commits.len(), 1);
        assert_eq!(git.commits[0].subject, "init");
        assert!(git.fetched_at > 1_700_000_000);

        // 文件树根是仓库子目录:仍从仓库根取状态,角标按绝对路径命中
        git.refresh(Some(&dir.join("docs")));
        assert_eq!(git.repo_root.as_deref(), Some(dir.as_path()));
        assert_eq!(git.badge_for(&dir.join("a.md")), Some(StatusKind::Modified));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 非 git 目录:降级文案 + 角标与历史清空(文件树无角标、Git 页提示)。
    #[test]
    fn refresh_on_non_git_dir_degrades() {
        let plain =
            std::env::temp_dir().join(format!("latermd-gitpanel-{}-plain", std::process::id()));
        let _ = std::fs::remove_dir_all(&plain);
        std::fs::create_dir_all(&plain).unwrap();

        let mut git = GitPanelState::default();
        git.refresh(Some(&plain));
        let error = git.error.as_deref().expect("降级文案");
        assert!(error.contains("不是"), "{error}");
        assert_eq!(git.repo_root, None);
        assert!(git.entries.is_empty() && git.badges.is_empty());
        assert!(git.commits.is_empty());

        // 从有状态降级:旧快照必须作废,不留误导性角标
        let dir = temp_repo("degrade-from");
        git.refresh(Some(&dir));
        assert!(git.badge_for(&dir.join("no-such.md")).is_none());
        let _ = std::fs::remove_dir_all(&plain);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 无根刷新:静默清空(无降级文案,由渲染层给「先选根目录」引导)。
    #[test]
    fn refresh_without_root_clears_silently() {
        let dir = temp_repo("rootless");
        let mut git = GitPanelState::default();
        git.refresh(Some(&dir));
        assert!(!git.entries.is_empty() || git.commits.is_empty() || git.badges.is_empty());

        git.refresh(None);
        assert_eq!(git.error, None, "无根不是错误");
        assert!(git.entries.is_empty() && git.badges.is_empty());
        assert!(git.commits.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 选中与 diff:列表内路径读出 diff 内容;列表外路径(过期消息)忽略。
    #[test]
    fn select_reads_diff_and_ignores_stale_paths() {
        let dir = temp_repo("select");
        std::fs::write(dir.join("a.md"), "HEAD 版本\n新行\n").unwrap();

        let mut git = GitPanelState::default();
        git.refresh(Some(&dir));
        git.select("a.md");
        assert_eq!(git.selected.as_deref(), Some("a.md"));
        assert!(git.diff.contains("+新行"), "{}", git.diff);

        git.select("not-in-list.md");
        assert_eq!(
            git.selected.as_deref(),
            Some("a.md"),
            "过期路径不顶掉选中项"
        );

        // 无改动的文件不在列表里,选不中;列表外的干净文件没有 diff 区
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 回滚链:request 只置模态;confirm 恢复 HEAD 并刷新(改动列表里
    /// 选中项随 clean 失效清空);cancel 不动文件。
    #[test]
    fn checkout_request_confirm_and_cancel() {
        let dir = temp_repo("checkout");
        std::fs::write(dir.join("a.md"), "工作区乱改\n").unwrap();

        let mut git = GitPanelState::default();
        git.refresh(Some(&dir));
        git.select("a.md");

        git.request_checkout("a.md".to_owned());
        assert_eq!(
            git.confirm_checkout.as_deref(),
            Some("a.md"),
            "请求只置模态"
        );
        assert!(
            std::fs::read_to_string(dir.join("a.md"))
                .unwrap()
                .contains("乱改"),
            "确认前文件未被触碰"
        );

        git.cancel_checkout();
        assert_eq!(git.confirm_checkout, None);
        assert!(std::fs::read_to_string(dir.join("a.md"))
            .unwrap()
            .contains("乱改"));

        git.request_checkout("a.md".to_owned());
        assert_eq!(git.confirm_checkout(Some(&dir)), None, "成功无错误文案");
        assert_eq!(
            std::fs::read_to_string(dir.join("a.md")).unwrap(),
            "HEAD 版本\n",
            "确认后恢复 HEAD"
        );
        assert_eq!(git.confirm_checkout, None, "模态已关闭");
        assert!(git.entries.is_empty(), "回滚后工作区干净");
        assert_eq!(git.selected, None, "选中项随 clean 失效清空");
        assert!(git.badges.is_empty(), "角标同步清空");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 到点与周期:refresh 顺延一个周期,due 到点翻转;until_refresh 单调递减。
    #[test]
    fn due_and_until_refresh_track_interval() {
        let dir = temp_repo("due");
        let mut git = GitPanelState::default();
        assert!(git.due(), "初始即到点(默认 Instant)");

        git.refresh(Some(&dir));
        assert!(!git.due(), "刷新后未到点");
        let first = git.until_refresh();
        assert!(first <= REFRESH_INTERVAL && first > REFRESH_INTERVAL / 2);
        std::thread::sleep(Duration::from_millis(20));
        assert!(git.until_refresh() < first, "剩余时长随时间递减");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
