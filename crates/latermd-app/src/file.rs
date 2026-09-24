//! 文件命令:新建 / 打开 / 保存 / 另存为(docs/roadmap.md P0「文件操作」)。
//!
//! 读写都是 UTF-8 字节原样进出:不做 CRLF↔LF 转换、不补尾换行
//! (roadmap P0 验收「`.md` 文件保持原样(无格式化篡改)」)。对话框用 rfd
//! 同步版,在 `App::logic` 的归约里弹出——原生模态对话框本来就会阻塞
//! 事件循环,弹框期间本应用没有需要继续绘制的状态。

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use eframe::egui::{self, Modifiers};

/// 对话框过滤器接受的 Markdown 扩展名(同步给 rfd,不带点)。
pub const MARKDOWN_EXTENSIONS: [&str; 2] = ["md", "markdown"];

/// 未落盘文档在另存为对话框里的预填文件名。
pub const UNTITLED_FILE_NAME: &str = "未命名.md";

/// 文件命令。UI 只产出,执行(弹框 + IO + 状态变更)在 [`crate::state::State::apply`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileCmd {
    /// 新建空文档。
    New,
    /// 打开已有文件。
    Open,
    /// 保存;从未落盘时等价于另存为。
    Save,
    /// 另存为(总是弹框)。
    SaveAs,
}

impl FileCmd {
    /// 工具栏按钮顺序。
    pub const ALL: [FileCmd; 4] = [Self::New, Self::Open, Self::Save, Self::SaveAs];

    /// 工具栏显示名。
    pub fn label(self) -> &'static str {
        match self {
            Self::New => "新建",
            Self::Open => "打开",
            Self::Save => "保存",
            Self::SaveAs => "另存为",
        }
    }

    /// 本模块绑定的快捷键:Ctrl/Cmd+S 与 Ctrl/Cmd+Shift+S。
    /// 完整快捷键表由后续模块统一接入,这里只认领保存两条。
    pub fn shortcut(self) -> Option<egui::KeyboardShortcut> {
        match self {
            Self::Save => Some(egui::KeyboardShortcut::new(
                Modifiers::COMMAND,
                egui::Key::S,
            )),
            Self::SaveAs => Some(egui::KeyboardShortcut::new(
                Modifiers::COMMAND | Modifiers::SHIFT,
                egui::Key::S,
            )),
            Self::New | Self::Open => None,
        }
    }
}

/// 从本帧输入消费文件快捷键,返回被触发的命令(已从输入流移除,不会重复触发)。
///
/// 消费顺序固定先 SaveAs 后 Save:`InputState::consume_shortcut` 按
/// `matches_logically` 匹配(多余 Shift 被忽略),先问 Save 的话
/// Ctrl+Shift+S 会先命中它。
pub fn poll_shortcuts(ctx: &egui::Context) -> Vec<FileCmd> {
    /// 消费顺序,见函数文档。
    const SHORTCUT_ORDER: [FileCmd; 2] = [FileCmd::SaveAs, FileCmd::Save];
    SHORTCUT_ORDER
        .iter()
        .filter_map(|cmd| {
            let shortcut = cmd.shortcut()?;
            ctx.input_mut(|input| input.consume_shortcut(&shortcut))
                .then_some(*cmd)
        })
        .collect()
}

/// 起始目录:当前文档所在目录;未落盘或路径无父目录时退回进程工作目录。
pub fn start_dir(current: Option<&Path>) -> PathBuf {
    current
        .and_then(Path::parent)
        // 相对文件名 "note.md" 的 parent 是 "",不是可用目录
        .filter(|dir| !dir.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_default()
}

/// 打开对话框,选一个已存在的 Markdown 文件;取消返回 `None`。
pub fn open_dialog(start_dir: &Path) -> Option<PathBuf> {
    markdown_dialog(start_dir).pick_file()
}

/// 另存为对话框,预填 `default_name`;取消返回 `None`。
pub fn save_dialog(start_dir: &Path, default_name: &str) -> Option<PathBuf> {
    markdown_dialog(start_dir)
        .set_file_name(default_name)
        .save_file()
}

fn markdown_dialog(start_dir: &Path) -> rfd::FileDialog {
    rfd::FileDialog::new()
        .add_filter("Markdown", &MARKDOWN_EXTENSIONS)
        .set_directory(start_dir)
}

/// 文件操作失败:带动作与路径,提示行可直接展示。
#[derive(Debug)]
pub struct FileError {
    op: &'static str,
    path: PathBuf,
    source: io::Error,
}

impl fmt::Display for FileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}失败 {}: {}",
            self.op,
            self.path.display(),
            self.source
        )
    }
}

fn err(op: &'static str, path: &Path, source: io::Error) -> FileError {
    FileError {
        op,
        path: path.to_path_buf(),
        source,
    }
}

/// UTF-8 读全文。非 UTF-8 文件直接报错而不是替换字符:替换后一旦保存,
/// 原编码内容就被永久毁掉;P0 明确只支持 UTF-8。
pub fn read(path: &Path) -> Result<String, FileError> {
    let bytes = std::fs::read(path).map_err(|source| err("打开", path, source))?;
    String::from_utf8(bytes).map_err(|_| {
        err(
            "打开",
            path,
            io::Error::new(io::ErrorKind::InvalidData, "不是 UTF-8 编码"),
        )
    })
}

/// UTF-8 写全文,字节原样落盘:不做换行符转换,不补尾换行。
pub fn write(path: &Path, text: &str) -> Result<(), FileError> {
    std::fs::write(path, text.as_bytes()).map_err(|source| err("保存", path, source))
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Key, RawInput};

    /// 进程内唯一且不冲突的临时路径;测试自删。
    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("latermd-file-{}-{name}", std::process::id()))
    }

    fn key_event(modifiers: Modifiers) -> Event {
        Event::Key {
            key: Key::S,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    /// 写出→读回逐字节一致:UTF-8、CRLF、LF 混排与尾随空行都原样保留。
    #[test]
    fn write_then_read_is_byte_exact() {
        let path = temp_path("roundtrip.md");
        let text = "# 标题\r\n\r\nCRLF 段落\nLF 段落\n\n";
        write(&path, text).unwrap();
        assert_eq!(read(&path).unwrap(), text);
        // 再对一次字节,确认没经过任何换行规范化
        assert_eq!(std::fs::read(&path).unwrap(), text.as_bytes());
        let _ = std::fs::remove_file(&path);
    }

    /// 非 UTF-8 文件报错并把路径带进提示,而不是静默替换成 U+FFFD。
    #[test]
    fn read_rejects_non_utf8_with_path_in_message() {
        let path = temp_path("gbk.md");
        std::fs::write(&path, [0xC4, 0xE3, 0xBA, 0xC3]).unwrap(); // GBK「你好」
        let error = read(&path).unwrap_err().to_string();
        assert!(error.contains("gbk.md"), "提示应含路径: {error}");
        assert!(error.contains("UTF-8"), "提示应说明原因: {error}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn read_missing_file_names_the_path() {
        let error = read(Path::new("/latermd/不存在.md"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("不存在.md"), "{error}");
    }

    /// Ctrl+Shift+S 只触发 SaveAs 一条;`matches_logically` 忽略多余 Shift,
    /// 若先消费 Save 会双触发(顺序约束见 [`poll_shortcuts`] 文档)。
    #[test]
    fn shift_save_fires_only_save_as() {
        let ctx = egui::Context::default();
        let output = ctx.run_ui(
            RawInput {
                events: vec![key_event(Modifiers::COMMAND | Modifiers::SHIFT)],
                ..Default::default()
            },
            |ui| {
                assert_eq!(poll_shortcuts(ui.ctx()), vec![FileCmd::SaveAs]);
            },
        );
        output.drop_without_applying_deltas();
    }

    #[test]
    fn plain_save_fires_only_save() {
        let ctx = egui::Context::default();
        let output = ctx.run_ui(
            RawInput {
                events: vec![key_event(Modifiers::COMMAND)],
                ..Default::default()
            },
            |ui| {
                assert_eq!(poll_shortcuts(ui.ctx()), vec![FileCmd::Save]);
            },
        );
        output.drop_without_applying_deltas();
    }

    /// 无修饰的 S 不是保存快捷键;同帧重复消费也不会二次返回。
    #[test]
    fn bare_s_does_not_fire() {
        let ctx = egui::Context::default();
        let output = ctx.run_ui(
            RawInput {
                events: vec![key_event(Modifiers::NONE)],
                ..Default::default()
            },
            |ui| {
                let ctx = ui.ctx().clone();
                assert!(poll_shortcuts(&ctx).is_empty());
                // 同一帧再问一次:已消费的按键不回流
                assert!(poll_shortcuts(&ctx).is_empty());
            },
        );
        output.drop_without_applying_deltas();
    }
}
