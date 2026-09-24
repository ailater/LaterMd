//! 文件树(P0 基础版,docs/roadmap.md):懒加载 + `.gitignore` + 大目录截断。
//!
//! 遍历用 `ignore::WalkBuilder` 且只列一层(`max_depth(1)`),天然遵循
//! `.gitignore`;目录展开时才发生 IO(懒加载),结果缓存在 `children`,
//! 稳态帧零文件系统调用。展开状态自管 `HashMap<PathBuf, bool>`,渲染用
//! `ui.indent` + `selectable_label` 而非 `CollapsingHeader`(ADR-005 §4.1:
//! 动态目录下后者的内部 ID 不可控)。
//!
//! 持久化与主题同思路:平台配置目录下手写 JSON(`file_tree.json`),serde
//! 往返、缺项回落默认;坏文件只告警不挡启动。

use std::collections::HashMap;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};

use crate::file::MARKDOWN_EXTENSIONS;
use crate::theme;

/// 持久化文件名,与 `settings.json` 同目录。
const SETTINGS_FILE: &str = "file_tree.json";

/// 单个目录在树里的直接子项显示上限(大目录保护:遍历整层收集计数,但
/// UI 只渲染前这么多项,剩余进截断提示行)。
pub const MAX_CHILDREN: usize = 500;

/// 最近目录保留条数。
pub const MAX_RECENTS: usize = 8;

/// 文件树运行态:根目录 + 展开状态 + 子项缓存(均为会话内状态,不持久化;
/// 持久化的是 [`FileTreeSettings`] 的根目录与最近列表)。
#[derive(Debug, Default)]
pub struct FileTreeState {
    /// 当前根目录;`None` = 未选择(Files 页显示引导)。
    pub root: Option<PathBuf>,
    /// 最近打开过的根目录(最新在前),供侧边栏快速切换。
    pub recents: Vec<PathBuf>,
    /// 目录展开状态(懒加载标志:置 `true` 后由 [`FileTreeState::ensure_loaded`]
    /// 在下一帧归约时列子项)。
    pub expanded: HashMap<PathBuf, bool>,
    /// 已列举的子项缓存。键缺席 + 对应目录展开中 = 待列举。
    pub children: HashMap<PathBuf, DirChildren>,
}

impl FileTreeState {
    /// 换根:root 生效、置顶去重进最近列表、丢弃全部展开与缓存(旧树的
    /// 展开状态对新根无意义)。
    pub fn set_root(&mut self, root: PathBuf) {
        self.recents.retain(|dir| *dir != root);
        self.recents.insert(0, root.clone());
        self.recents.truncate(MAX_RECENTS);
        self.root = Some(root);
        self.expanded.clear();
        self.children.clear();
    }

    /// 翻转目录展开状态(点击目录行)。
    pub fn toggle(&mut self, dir: &Path) {
        let open = self.expanded.entry(dir.to_path_buf()).or_insert(false);
        *open = !*open;
    }

    /// 展开文件在树内的全部祖先目录(文件打开后高亮行可见的前提);
    /// 文件不在当前根之下则不动。
    pub fn expand_ancestors_of(&mut self, file: &Path) {
        let Some(root) = self.root.as_deref() else {
            return;
        };
        let mut dir = file.parent();
        while let Some(current) = dir {
            if current == root || !current.starts_with(root) {
                break;
            }
            self.expanded.insert(current.to_path_buf(), true);
            dir = current.parent();
        }
    }

    /// 补齐「根目录 + 展开中目录」的子项缓存。每帧归约末尾调用:键缺席才
    /// 发生 IO,稳态零开销;这正是懒加载的落点(点击目录只翻转标志,列举
    /// 延迟到下一次归约)。
    pub fn ensure_loaded(&mut self) {
        let Self {
            root,
            expanded,
            children,
            ..
        } = self;
        if let Some(root) = root {
            ensure_children(children, root);
        }
        for (dir, open) in expanded {
            if *open {
                ensure_children(children, dir);
            }
        }
    }
}

/// 缓存缺席才列举。
fn ensure_children(children: &mut HashMap<PathBuf, DirChildren>, dir: &Path) {
    children
        .entry(dir.to_path_buf())
        .or_insert_with(|| list_children(dir, MAX_CHILDREN));
}

/// 一个目录的直接子项(已过滤排序截断)。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DirChildren {
    /// 可见子项:目录在前、名称不区分大小写字典序。
    pub entries: Vec<TreeEntry>,
    /// 超过 [`MAX_CHILDREN`] 被截断的子项数(截断提示行数据源)。
    pub truncated: usize,
}

/// 树里的一行:目录或 Markdown 文件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    pub path: PathBuf,
    /// 显示名(目录名或文件名,含扩展名)。
    pub name: String,
    pub is_dir: bool,
}

/// 列举一个目录的直接子项(`max_depth(1)` 语义):
///
/// - 遵循 `.gitignore` 与隐藏文件过滤(`WalkBuilder` 默认;`require_git(false)`
///   让无 `.git` 的普通目录也吃到自己写的 `.gitignore`,符合知识库直觉);
/// - 只保留目录与 `.md` / `.markdown` 文件;
/// - 目录在前、名称不区分大小写字典序;超出 `cap` 的截断并计数。
///
/// `cap` 参数化只为测试可注入小值;生产恒为 [`MAX_CHILDREN`]。
pub fn list_children(dir: &Path, cap: usize) -> DirChildren {
    let mut dirs: Vec<TreeEntry> = Vec::new();
    let mut files: Vec<TreeEntry> = Vec::new();
    // 遍历错误(权限、目录消失)跳过该条目:树是导航手段,不因个别
    // 条目不可读而整树失败
    for entry in WalkBuilder::new(dir)
        .max_depth(Some(1))
        .require_git(false)
        .build()
        .filter_map(Result::ok)
    {
        // depth 0 是根自身;max_depth(1) 之下其余即直接子项
        if entry.depth() == 0 {
            continue;
        }
        let is_dir = entry.file_type().is_some_and(|kind| kind.is_dir());
        if !is_dir && !is_markdown(entry.path()) {
            continue;
        }
        let item = TreeEntry {
            path: entry.path().to_path_buf(),
            name: entry.file_name().to_string_lossy().into_owned(),
            is_dir,
        };
        (if is_dir { &mut dirs } else { &mut files }).push(item);
    }
    let by_name = |a: &TreeEntry, b: &TreeEntry| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.name.cmp(&b.name))
    };
    dirs.sort_by(by_name);
    files.sort_by(by_name);
    dirs.append(&mut files);
    let truncated = dirs.len().saturating_sub(cap);
    dirs.truncate(cap);
    DirChildren {
        entries: dirs,
        truncated,
    }
}

/// 扩展名是否 Markdown(与打开对话框的过滤器同一张清单)。
fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            MARKDOWN_EXTENSIONS
                .iter()
                .any(|known| ext.eq_ignore_ascii_case(known))
        })
}

/// 持久化的文件树设置(根目录 + 最近列表)。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FileTreeSettings {
    /// 上次会话的根目录,启动即恢复。
    pub root: Option<PathBuf>,
    /// 最近根目录列表(最新在前)。
    pub recents: Vec<PathBuf>,
}

impl FileTreeSettings {
    /// 启动装载:无文件用默认;坏文件终端告警后回落默认(挡启动不值得)。
    pub fn load() -> Self {
        let Some(dir) = theme::config_dir() else {
            return Self::default();
        };
        match Self::load_from(&dir) {
            Ok(settings) => settings,
            Err(LoadError::Missing) => Self::default(),
            Err(LoadError::Corrupt(source)) => {
                eprintln!("LaterMD: 文件树设置解析失败,已回落默认: {source}");
                Self::default()
            }
        }
    }

    /// 落盘到 `<dir>/file_tree.json`;`dir` 为 `None` 时用平台默认目录。
    /// 目录不存在则创建。失败带路径,提示行可直接展示。
    pub fn save_to(&self, dir: Option<&Path>) -> Result<(), SaveError> {
        let Some(dir) = dir.map(Path::to_path_buf).or_else(theme::config_dir) else {
            return Err(SaveError {
                path: PathBuf::from(SETTINGS_FILE),
                source: "找不到平台配置目录(HOME/APPDATA 均未设置)".into(),
            });
        };
        let json = serde_json::to_string_pretty(self).map_err(|source| SaveError {
            path: dir.join(SETTINGS_FILE),
            source: Box::new(source),
        })?;
        std::fs::create_dir_all(&dir).map_err(|source| SaveError {
            path: dir.clone(),
            source: Box::new(source),
        })?;
        std::fs::write(dir.join(SETTINGS_FILE), json.as_bytes()).map_err(|source| SaveError {
            path: dir.join(SETTINGS_FILE),
            source: Box::new(source),
        })
    }

    fn load_from(dir: &Path) -> Result<Self, LoadError> {
        let path = dir.join(SETTINGS_FILE);
        let bytes = std::fs::read(&path).map_err(|source| match source.kind() {
            io::ErrorKind::NotFound => LoadError::Missing,
            _ => LoadError::Corrupt(format!("{}: {}", path.display(), source)),
        })?;
        serde_json::from_slice(&bytes)
            .map_err(|source| LoadError::Corrupt(format!("{}: {}", path.display(), source)))
    }
}

impl From<&FileTreeState> for FileTreeSettings {
    fn from(state: &FileTreeState) -> Self {
        Self {
            root: state.root.clone(),
            recents: state.recents.clone(),
        }
    }
}

impl From<FileTreeSettings> for FileTreeState {
    /// 启动恢复:只取根目录与最近列表,展开状态从零开始(根的一级子项由
    /// 首帧 `ensure_loaded` 列出)。
    fn from(settings: FileTreeSettings) -> Self {
        Self {
            root: settings.root,
            recents: settings.recents,
            ..Self::default()
        }
    }
}

/// 读设置的失败情形(与 `crate::theme` 同构)。
#[derive(Debug)]
enum LoadError {
    Missing,
    Corrupt(String),
}

/// 写设置失败:带路径,可直接进提示行。
#[derive(Debug)]
pub struct SaveError {
    path: PathBuf,
    source: Box<dyn std::error::Error + Send + Sync>,
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "文件树设置保存失败 {}: {}",
            self.path.display(),
            self.source
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 进程内唯一的临时目录;测试自删。
    fn temp_tree(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("latermd-filetree-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(root: &Path, name: &str) {
        std::fs::write(root.join(name), b"x").unwrap();
    }

    fn entry_names(children: &DirChildren) -> Vec<&str> {
        children.entries.iter().map(|e| e.name.as_str()).collect()
    }

    /// 过滤与排序:目录在前(不区分大小写字典序),文件只留 Markdown;
    /// `.gitignore` 条目与隐藏文件不出现(`require_git(false)` 让无 `.git`
    /// 的目录同样吃 `.gitignore`)。
    #[test]
    fn list_children_keeps_dirs_and_markdown_only() {
        let dir = temp_tree("list");
        std::fs::write(dir.join(".gitignore"), b"node_modules/\n").unwrap();
        for name in ["b.md", "z.md", "c.txt", ".hidden.md", "a.markdown", "A.MD"] {
            touch(&dir, name);
        }
        std::fs::create_dir(dir.join("node_modules")).unwrap();
        touch(&dir.join("node_modules"), "inner.md");
        std::fs::create_dir(dir.join("docs")).unwrap();
        touch(&dir.join("docs"), "nested.md");

        let children = list_children(&dir, MAX_CHILDREN);
        // 大小写不敏感字典序:a.markdown < A.MD("a" < "d" 于第 4 字符)
        assert_eq!(
            entry_names(&children),
            ["docs", "a.markdown", "A.MD", "b.md", "z.md"]
        );
        assert_eq!(children.truncated, 0);
        // 深层文件没有出现:确实只有一层
        assert!(!children.entries.iter().any(|e| e.name == "nested.md"));
        // 目录与文件的分组标记供渲染区分点击行为
        assert!(children.entries[0].is_dir);
        assert!(!children.entries[1].is_dir);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 截断:超出 cap 的子项计数进 `truncated`,排序在前 cap 项保留。
    #[test]
    fn list_children_truncates_beyond_cap() {
        let dir = temp_tree("truncate");
        std::fs::create_dir(dir.join("dir")).unwrap();
        for index in 0..5 {
            touch(&dir, &format!("f{index}.md"));
        }

        let children = list_children(&dir, 3);
        assert_eq!(children.entries.len(), 3, "目录在前占首位");
        assert_eq!(entry_names(&children), ["dir", "f0.md", "f1.md"]);
        assert_eq!(children.truncated, 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 不存在的目录列举为空而不是 panic(外部删除的目录在树里自然消失)。
    #[test]
    fn list_children_of_missing_dir_is_empty() {
        let children = list_children(Path::new("/latermd/no/such/dir"), MAX_CHILDREN);
        assert!(children.entries.is_empty());
        assert_eq!(children.truncated, 0);
    }

    /// 换根:最近列表置顶去重并截断;展开与缓存全部丢弃。
    #[test]
    fn set_root_refreshes_recents_and_drops_caches() {
        let mut tree = FileTreeState::default();
        let a = PathBuf::from("/vault/a");
        let b = PathBuf::from("/vault/b");
        tree.set_root(a.clone());
        tree.set_root(b.clone());
        tree.set_root(a.clone()); // 重复选择 → 置顶而不是重复入列

        assert_eq!(tree.root.as_deref(), Some(a.as_path()));
        assert_eq!(tree.recents, vec![a.clone(), b]);

        tree.toggle(&a);
        tree.children.insert(
            a.join("sub"),
            DirChildren {
                entries: vec![TreeEntry {
                    path: a.join("sub/x.md"),
                    name: "x.md".into(),
                    is_dir: false,
                }],
                truncated: 0,
            },
        );
        let c = PathBuf::from("/vault/c");
        tree.set_root(c.clone());
        assert_eq!(tree.root.as_deref(), Some(c.as_path()));
        assert!(tree.expanded.is_empty(), "旧根的展开状态不带入新根");
        assert!(tree.children.is_empty(), "旧根的子项缓存不带入新根");
    }

    /// 祖先展开:只展开根之内的目录,根自身不需要标志;根外文件不动。
    #[test]
    fn expand_ancestors_only_within_root() {
        let mut tree = FileTreeState::default();
        tree.set_root(PathBuf::from("/vault"));
        tree.expand_ancestors_of(Path::new("/vault/docs/deep/note.md"));
        assert_eq!(
            tree.expanded,
            HashMap::from([
                (PathBuf::from("/vault/docs"), true),
                (PathBuf::from("/vault/docs/deep"), true),
            ])
        );

        tree.expand_ancestors_of(Path::new("/elsewhere/note.md"));
        assert_eq!(tree.expanded.len(), 2, "根外文件的祖先不进展开表");

        let mut bare = FileTreeState::default();
        bare.expand_ancestors_of(Path::new("/x/y.md"));
        assert!(bare.expanded.is_empty(), "无根时无操作");
    }

    /// 懒加载落点:`ensure_loaded` 只补「根 + 展开中」目录的缓存,缺席才 IO;
    /// 已缓存(含空目录)的条目不再动。
    #[test]
    fn ensure_loaded_lists_only_missing_expanded_dirs() {
        let root = temp_tree("lazy");
        std::fs::create_dir(root.join("docs")).unwrap();
        touch(&root.join("docs"), "a.md");
        touch(&root, "top.md");

        let mut tree = FileTreeState::default();
        tree.set_root(root.clone());
        assert!(tree.children.is_empty(), "set_root 不列举,列举延迟到归约");

        tree.ensure_loaded();
        let root_children = tree.children.get(&root).expect("根子项已列举");
        assert_eq!(entry_names(root_children), ["docs", "top.md"]);

        tree.toggle(&root.join("docs"));
        tree.ensure_loaded();
        let docs = tree
            .children
            .get(root.join("docs").as_path())
            .expect("展开后才列举");
        assert_eq!(entry_names(docs), ["a.md"]);

        // 未展开目录不列举;再跑一遍不新增条目(稳态零 IO 的缓存语义)
        assert!(!tree.children.contains_key(&root.join("nope")));
        let before = tree.children.len();
        tree.ensure_loaded();
        assert_eq!(tree.children.len(), before);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// 持久化往返:根与最近列表逐项一致;`FileTreeState` 双向转换只带这两项。
    #[test]
    fn settings_round_trip_and_state_conversion() {
        let dir = temp_tree("settings");
        let settings = FileTreeSettings {
            root: Some(PathBuf::from("/vault/docs")),
            recents: vec![PathBuf::from("/vault/docs"), PathBuf::from("/notes")],
        };
        settings.save_to(Some(&dir)).unwrap();
        assert_eq!(FileTreeSettings::load_from(&dir).unwrap(), settings);
        let _ = std::fs::remove_dir_all(&dir);

        let tree = FileTreeState::from(settings.clone());
        assert_eq!(tree.root, settings.root);
        assert_eq!(tree.recents, settings.recents);
        assert!(tree.expanded.is_empty() && tree.children.is_empty());
        assert_eq!(FileTreeSettings::from(&tree), settings);
    }

    /// 手改配置只留一个键:serde(default) 缺项回落,不整体失败。
    #[test]
    fn partial_json_fills_defaults() {
        let dir = temp_tree("partial");
        std::fs::write(dir.join(SETTINGS_FILE), br#"{"root":"/vault"}"#).unwrap();
        assert_eq!(
            FileTreeSettings::load_from(&dir).unwrap(),
            FileTreeSettings {
                root: Some(PathBuf::from("/vault")),
                recents: Vec::new(),
            }
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
