#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! 只读 Git 能力(git2 封装,docs/roadmap.md 阶段 4「P2 版本层」)。
//!
//! 定位:以仓库根目录为参数的纯函数集合,供 UI 层查询状态、历史、diff、
//! blame,外加唯一一个写操作——丢弃工作区改动、恢复 HEAD 版本的单文件
//! checkout。不依赖 egui(铁律:业务逻辑不依赖 UI 框架,AGENTS.md §3)。
//!
//! 口径:
//! * 所有函数接受**仓库根**(.git 所在目录),不向上层目录探测;传入非
//!   git 目录一律返回 Err,由调用方降级为面板提示,绝不 panic。
//! * 未提交的工作区改动参与 status 与 diff;blame 只基于 HEAD 提交内容
//!   (libgit2 限制,工作区未提交的行不参与行级归属)。
//! * 错误类型是面向用户的中文描述,可直接上提示行。
//! * 不做 remote 操作(fetch/push/pull 不在 P2 范围),故 git2 关掉了
//!   ssh/https 传输 feature。

use std::fmt;
use std::path::{Path, PathBuf};

use git2::build::CheckoutBuilder;
use git2::{DiffOptions, Patch, Repository, Signature, StatusOptions};

/// diff 输出的字节上限(截断保护,~64KB)。
const MAX_DIFF_BYTES: usize = 64 * 1024;
/// 截断时追加的提示行(与 MAX_DIFF_BYTES 对应,改上限记得同步文案)。
const TRUNCATION_NOTICE: &str = "\n…(diff 超过 64KB,已截断)\n";
/// pathspec 命中二进制文件(无文本 diff)时的占位输出。
const BINARY_NOTICE: &str = "(二进制文件,无文本 diff)\n";

/// 单文件状态码,与 `git status --short` 语义一致:
/// `M` 已修改、`A` 已暂存新增、`U` 未合并(冲突)、`D` 已删除、`?` 未跟踪。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum StatusKind {
    /// 工作区或暂存区内容有修改(含重命名、类型变更)。
    Modified,
    /// 已 add 进暂存区的新文件。
    Added,
    /// 合并冲突中(unmerged)。
    Unmerged,
    /// 已删除(暂存区或工作区)。
    Deleted,
    /// 未跟踪的新文件(untracked)。
    Untracked,
}

impl fmt::Display for StatusKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let letter = match self {
            StatusKind::Modified => 'M',
            StatusKind::Added => 'A',
            StatusKind::Unmerged => 'U',
            StatusKind::Deleted => 'D',
            StatusKind::Untracked => '?',
        };
        write!(f, "{letter}")
    }
}

/// 一个文件的状态条目。
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FileStatus {
    /// 相对仓库根的路径,POSIX 分隔符。
    pub path: String,
    /// 状态码。
    pub code: StatusKind,
}

/// 一条提交记录。
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CommitInfo {
    /// 完整 hash(40 位十六进制)。
    pub hash: String,
    /// 短 hash(与 git 默认 core.abbrev 一致的 7 位,不做仓库内唯一性消歧)。
    pub short_hash: String,
    /// 首行提交说明。
    pub subject: String,
    /// 作者,`名字 <邮箱>` 格式(缺失部分自动省略)。
    pub author: String,
    /// 提交时间,Unix 秒(UI 层自行格式化为本地时区)。
    pub time: i64,
}

/// 一行的 blame 归属。
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct BlameLine {
    /// 行号,从 1 开始。
    pub line_no: u32,
    /// 该行最后修改提交的短 hash。
    pub short_hash: String,
    /// 该提交的首行说明。
    pub subject: String,
}

/// [`log`] 的默认条数上限。
pub const DEFAULT_LOG_LIMIT: usize = 50;

/// 从任意目录向上探测 Git 仓库的工作区根(`.git` 所在目录),起点自身
/// 是仓库根也命中。UI 的文件树根可能是仓库子目录(例如选了 `docs/`),
/// 其余 API 只认仓库根,先经本函数换算。裸仓库(只有 `.git` 内容、无
/// 工作区)返回 Err——本 crate 的全部操作都针对工作区文件。
pub fn discover(start: &Path) -> Result<PathBuf, String> {
    Repository::discover(start)
        .map_err(|error| format!("当前目录不是 Git 仓库: {error}"))
        .and_then(|repo| {
            repo.workdir()
                .map(Path::to_path_buf)
                .ok_or_else(|| "当前是裸仓库,没有工作区文件".to_owned())
        })
}

/// 读取仓库全部改动(含未跟踪文件),按路径排序。
///
/// 未跟踪目录会递归展开到逐个文件,便于文件树打标;`.gitignore` 命中的
/// 文件按 git 惯例不出现;非 UTF-8 文件名同样不出现(极罕见,libgit2
/// 拿不到 &str 形式的路径)。
pub fn status(root: &Path) -> Result<Vec<FileStatus>, String> {
    let repo = open_repo(root)?;
    let mut options = StatusOptions::new();
    options.include_untracked(true).recurse_untracked_dirs(true);
    let statuses = repo
        .statuses(Some(&mut options))
        .map_err(|error| format!("读取仓库状态失败: {error}"))?;
    let mut entries: Vec<FileStatus> = statuses
        .iter()
        .filter_map(|entry| {
            entry.path().ok().map(|path| FileStatus {
                path: path.to_owned(),
                code: status_kind(entry.status()),
            })
        })
        .collect();
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

/// 近期提交列表,新提交在前,最多 `limit` 条;还没有任何提交的空仓库
/// 返回空列表而不是错误。排序取「拓扑 + 时间」组合:拓扑保证 parent
/// 永远排在 child 之后,时间解决同层先后(纯时间排序在同一秒内的多笔
/// 提交间顺序不稳定)。
pub fn log(root: &Path, limit: usize) -> Result<Vec<CommitInfo>, String> {
    let repo = open_repo(root)?;
    if repo
        .is_empty()
        .map_err(|error| format!("检查仓库状态失败: {error}"))?
    {
        return Ok(Vec::new());
    }
    let mut revwalk = repo
        .revwalk()
        .map_err(|error| format!("读取提交历史失败: {error}"))?;
    revwalk
        .push_head()
        .map_err(|error| format!("读取提交历史失败: {error}"))?;
    revwalk
        .set_sorting(git2::Sort::TIME | git2::Sort::TOPOLOGICAL)
        .map_err(|error| format!("读取提交历史失败: {error}"))?;
    let mut commits = Vec::new();
    for oid in revwalk.take(limit) {
        let oid = oid.map_err(|error| format!("遍历提交历史失败: {error}"))?;
        let commit = repo
            .find_commit(oid)
            .map_err(|error| format!("读取提交失败: {error}"))?;
        commits.push(CommitInfo {
            hash: commit.id().to_string(),
            short_hash: short_hash(&commit)?,
            subject: commit.summary().ok().flatten().unwrap_or("").to_owned(),
            author: signature_display(&commit.author()),
            time: commit.time().seconds(),
        });
    }
    Ok(commits)
}

/// 单文件 HEAD 与工作区(含暂存区)的 unified diff。
///
/// 无改动、文件不在 HEAD 与工作区时返回空串,由调用方显示「无改动」;
/// 空仓库(没有 HEAD 提交)返回 Err。未跟踪文件不参与 diff(与
/// `git diff` 一致)。pathspec 命中二进制文件时输出占位提示;输出超过
/// ~64KB 上限时截断并追加提示行。
pub fn diff_file(root: &Path, path: &str) -> Result<String, String> {
    let repo = open_repo(root)?;
    if repo
        .is_empty()
        .map_err(|error| format!("检查仓库状态失败: {error}"))?
    {
        return Err("仓库还没有任何提交,没有 HEAD 可对比".to_owned());
    }
    let tree = repo
        .head()
        .and_then(|head| head.peel_to_tree())
        .map_err(|error| format!("读取 HEAD 失败: {error}"))?;
    let mut options = DiffOptions::new();
    options.pathspec(path);
    let diff = repo
        .diff_tree_to_workdir_with_index(Some(&tree), Some(&mut options))
        .map_err(|error| format!("计算 diff 失败: {error}"))?;
    let mut text = String::new();
    for index in 0..diff.deltas().len() {
        match Patch::from_diff(&diff, index).map_err(|error| format!("生成 diff 失败: {error}"))?
        {
            // None = 该 delta 无文本 diff(二进制文件)
            None => text.push_str(BINARY_NOTICE),
            Some(mut patch) => {
                let buf = patch
                    .to_buf()
                    .map_err(|error| format!("生成 diff 失败: {error}"))?;
                text.push_str(&String::from_utf8_lossy(&buf));
            }
        }
    }
    Ok(truncate_diff(&text))
}

/// 单文件的行级归属,基于 HEAD 提交内容;未提交的工作区行不参与
/// (libgit2 的 blame 不看工作区)。文件在 HEAD 中不存在(未跟踪或
/// 未提交过)返回 Err。
pub fn blame_file(root: &Path, path: &str) -> Result<Vec<BlameLine>, String> {
    let repo = open_repo(root)?;
    let blame = repo
        .blame_file(Path::new(path), None)
        .map_err(|error| format!("blame 失败: {error}"))?;
    let mut lines = Vec::new();
    let mut line_no: usize = 1;
    while let Some(hunk) = blame.get_line(line_no) {
        let commit_id = hunk.final_commit_id();
        let (short_hash, subject) = if commit_id.is_zero() {
            // libgit2 理论上不产出未提交行,防御性兜底
            ("0".repeat(7), "未提交的改动".to_owned())
        } else {
            let commit = repo
                .find_commit(commit_id)
                .map_err(|error| format!("读取 blame 提交失败: {error}"))?;
            (
                short_hash(&commit)?,
                commit.summary().ok().flatten().unwrap_or("").to_owned(),
            )
        };
        lines.push(BlameLine {
            line_no: line_no as u32,
            short_hash,
            subject,
        });
        line_no += 1;
    }
    Ok(lines)
}

/// 丢弃该文件的工作区改动,恢复为 HEAD 版本(等价 `git checkout -- <path>`,
/// 已删除的文件同样会被恢复)。
///
/// **本 crate 唯一的写操作,且不可逆(改动没有回收站):必须由 UI 侧弹
/// 确认模态、用户显式确认之后才允许调用。**
///
/// `path` 按 git pathspec 语义匹配;对未跟踪文件调用是无害的空操作。
pub fn checkout_file(root: &Path, path: &str) -> Result<(), String> {
    let repo = open_repo(root)?;
    let mut builder = CheckoutBuilder::new();
    builder.path(path).force();
    repo.checkout_head(Some(&mut builder))
        .map_err(|error| format!("恢复文件失败: {error}"))
}

/// 打开仓库根;非 git 目录返回面向用户的错误。
fn open_repo(root: &Path) -> Result<Repository, String> {
    Repository::open(root).map_err(|error| format!("不是可用的 Git 仓库: {error}"))
}

/// libgit2 状态标志收敛为单字母状态码;同一文件多种标志并存时,
/// 冲突 > 未跟踪 > 暂存新增 > 删除 > 修改。
fn status_kind(flags: git2::Status) -> StatusKind {
    if flags.contains(git2::Status::CONFLICTED) {
        StatusKind::Unmerged
    } else if flags.contains(git2::Status::WT_NEW) {
        StatusKind::Untracked
    } else if flags.contains(git2::Status::INDEX_NEW) {
        StatusKind::Added
    } else if flags.intersects(git2::Status::INDEX_DELETED | git2::Status::WT_DELETED) {
        StatusKind::Deleted
    } else {
        StatusKind::Modified
    }
}

/// 提交作者的单行展示;名字或邮箱缺失时省略对应部分。
fn signature_display(signature: &Signature<'_>) -> String {
    match (signature.name().ok(), signature.email().ok()) {
        (Some(name), Some(email)) => format!("{name} <{email}>"),
        (Some(name), None) => name.to_owned(),
        (None, Some(email)) => email.to_owned(),
        (None, None) => String::new(),
    }
}

/// 提交的短 hash(libgit2 默认 core.abbrev,一般 7 位)。
fn short_hash(commit: &git2::Commit<'_>) -> Result<String, String> {
    commit
        .as_object()
        .short_id()
        .map_err(|error| format!("生成短 hash 失败: {error}"))
        .map(|buf| String::from_utf8_lossy(&buf).into_owned())
}

/// 超过 [`MAX_DIFF_BYTES`] 时在字符边界截断并追加提示行。
fn truncate_diff(text: &str) -> String {
    if text.len() <= MAX_DIFF_BYTES {
        return text.to_owned();
    }
    let mut cut = MAX_DIFF_BYTES;
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    let mut truncated = text[..cut].to_owned();
    truncated.push_str(TRUNCATION_NOTICE);
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// 建一次性临时仓库(每次全新目录,测试间无共享状态)。fixture 用
    /// 系统 git CLI 装配(与 latermd-app 的 git_diff.rs 同模式),被测
    /// API 全部走 latermd-git 自身。
    fn temp_repo(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("latermd-gitcrate-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        run_git(&dir, &["init", "-q"]);
        dir
    }

    /// 在临时仓库里跑 git;失败即 panic(fixture 装配错误)。
    fn run_git(dir: &Path, args: &[&str]) {
        let output = std::process::Command::new("git")
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

    /// 全部暂存并提交一条。
    fn commit_all(dir: &Path, message: &str) {
        run_git(dir, &["add", "."]);
        run_git(dir, &["commit", "-q", "-m", message]);
    }

    /// 四种日常状态各自成码,结果按路径排序。
    #[test]
    fn status_reports_codes_sorted_by_path() {
        let dir = temp_repo("status");
        std::fs::write(dir.join("a.md"), "一\n").unwrap();
        std::fs::write(dir.join("b.md"), "二\n").unwrap();
        commit_all(&dir, "init");

        std::fs::write(dir.join("a.md"), "一\n改\n").unwrap(); // M
        std::fs::write(dir.join("new.md"), "新\n").unwrap();
        run_git(&dir, &["add", "new.md"]); // A
        std::fs::remove_file(dir.join("b.md")).unwrap(); // D
        std::fs::write(dir.join("untracked.md"), "?\n").unwrap(); // ?

        let pairs: Vec<(String, String)> = status(&dir)
            .unwrap()
            .into_iter()
            .map(|entry| (entry.path, entry.code.to_string()))
            .collect();
        assert_eq!(
            pairs,
            vec![
                ("a.md".to_owned(), "M".to_owned()),
                ("b.md".to_owned(), "D".to_owned()),
                ("new.md".to_owned(), "A".to_owned()),
                ("untracked.md".to_owned(), "?".to_owned()),
            ],
            "{pairs:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 历史新提交在前,limit 生效,元数据完整。
    #[test]
    fn log_lists_newest_first_with_metadata() {
        let dir = temp_repo("log");
        std::fs::write(dir.join("a.md"), "一\n").unwrap();
        commit_all(&dir, "第一笔");
        std::fs::write(dir.join("a.md"), "一\n二\n").unwrap();
        commit_all(&dir, "第二笔");
        std::fs::write(dir.join("a.md"), "一\n二\n三\n").unwrap();
        commit_all(&dir, "第三笔");

        let commits = log(&dir, 2).unwrap();
        assert_eq!(commits.len(), 2, "limit 生效");
        assert_eq!(commits[0].subject, "第三笔");
        assert_eq!(commits[1].subject, "第二笔");
        assert_eq!(commits[0].hash.len(), 40);
        assert!(commits[0].short_hash.len() >= 7);
        assert!(
            commits[0].hash.starts_with(&commits[0].short_hash),
            "short_hash 必须是 hash 前缀"
        );
        assert!(
            commits[0].author.contains("LaterMD"),
            "{}",
            commits[0].author
        );
        assert!(commits[0].time >= commits[1].time, "新提交不早于旧提交");

        assert!(log(&dir, 0).unwrap().is_empty());
        assert_eq!(log(&dir, DEFAULT_LOG_LIMIT).unwrap().len(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 没有任何提交的仓库:历史为空列表而非错误。
    #[test]
    fn log_on_repo_without_commits_is_empty() {
        let dir = temp_repo("empty-log");
        assert!(log(&dir, DEFAULT_LOG_LIMIT).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// diff 是 HEAD 对工作区;干净文件返回空串。
    #[test]
    fn diff_file_shows_head_vs_workdir_and_clean_is_empty() {
        let dir = temp_repo("diff");
        std::fs::write(dir.join("a.md"), "旧\n").unwrap();
        std::fs::write(dir.join("clean.md"), "不变\n").unwrap();
        commit_all(&dir, "init");

        assert_eq!(diff_file(&dir, "clean.md").unwrap(), "", "无改动是空串");

        std::fs::write(dir.join("a.md"), "旧\n新行\n").unwrap();
        let diff = diff_file(&dir, "a.md").unwrap();
        assert!(diff.contains("+++ b/a.md"), "{diff}");
        assert!(diff.contains("+新行"), "{diff}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 病态大 diff 在 ~64KB 截断。
    #[test]
    fn diff_file_truncates_near_64kb() {
        let dir = temp_repo("diff-cap");
        std::fs::write(dir.join("big.md"), "小\n").unwrap();
        commit_all(&dir, "init");
        std::fs::write(dir.join("big.md"), "很长的中文内容。".repeat(50_000)).unwrap();

        let diff = diff_file(&dir, "big.md").unwrap();
        assert!(
            diff.len() <= MAX_DIFF_BYTES + TRUNCATION_NOTICE.len(),
            "必须截断: {}",
            diff.len()
        );
        assert!(diff.contains("已截断"), "{diff}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// blame 行级归属各自的提交;未提交过的文件是显式 Err。
    #[test]
    fn blame_attributes_lines_to_their_commits() {
        let dir = temp_repo("blame");
        std::fs::write(dir.join("doc.md"), "第一行\n").unwrap();
        commit_all(&dir, "初稿");
        std::fs::write(dir.join("doc.md"), "第一行\n第二行\n").unwrap();
        commit_all(&dir, "追加第二行");

        let commits = log(&dir, DEFAULT_LOG_LIMIT).unwrap();
        let newest = &commits[0]; // 追加第二行
        let oldest = &commits[1]; // 初稿

        let lines = blame_file(&dir, "doc.md").unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].line_no, 1);
        assert_eq!(lines[0].short_hash, oldest.short_hash);
        assert_eq!(lines[0].subject, "初稿");
        assert_eq!(lines[1].line_no, 2);
        assert_eq!(lines[1].short_hash, newest.short_hash);
        assert_eq!(lines[1].subject, "追加第二行");

        std::fs::write(dir.join("fresh.md"), "新\n").unwrap();
        assert!(blame_file(&dir, "fresh.md").is_err(), "未提交文件无 blame");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// checkout 恢复被修改与被删除的文件到 HEAD 版本。
    #[test]
    fn checkout_file_restores_head_version() {
        let dir = temp_repo("checkout");
        std::fs::write(dir.join("a.md"), "HEAD 版本\n").unwrap();
        commit_all(&dir, "init");

        std::fs::write(dir.join("a.md"), "工作区乱改\n").unwrap();
        checkout_file(&dir, "a.md").unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("a.md")).unwrap(),
            "HEAD 版本\n"
        );

        std::fs::remove_file(dir.join("a.md")).unwrap();
        checkout_file(&dir, "a.md").unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("a.md")).unwrap(),
            "HEAD 版本\n",
            "删除的文件同样可恢复"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 非 git 目录:全部 API 返回 Err,不 panic(优雅降级的前提)。
    #[test]
    fn non_git_directory_degrades_to_error_not_panic() {
        let dir =
            std::env::temp_dir().join(format!("latermd-gitcrate-{}-plain", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        assert!(status(&dir).is_err());
        assert!(log(&dir, DEFAULT_LOG_LIMIT).is_err());
        assert!(diff_file(&dir, "a.md").is_err());
        assert!(blame_file(&dir, "a.md").is_err());
        assert!(checkout_file(&dir, "a.md").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// discover 从仓库子目录向上找到工作区根;裸仓库没有工作区,显式 Err。
    #[test]
    fn discover_walks_up_to_workdir_root() {
        let dir = temp_repo("discover");
        std::fs::create_dir_all(dir.join("docs/deep")).unwrap();
        std::fs::write(dir.join("docs/deep/a.md"), "一\n").unwrap();

        assert_eq!(discover(&dir).unwrap(), dir, "起点即仓库根");
        assert_eq!(
            discover(&dir.join("docs/deep")).unwrap(),
            dir,
            "子目录向上探测"
        );

        let bare =
            std::env::temp_dir().join(format!("latermd-gitcrate-{}-bare", std::process::id()));
        let _ = std::fs::remove_dir_all(&bare);
        std::fs::create_dir_all(&bare).unwrap();
        run_git(&bare, &["init", "-q", "--bare", "."]);
        assert!(discover(&bare).is_err(), "裸仓库无工作区");
        let _ = std::fs::remove_dir_all(&bare);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
