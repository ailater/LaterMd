//! 文件命令:新建 / 打开 / 保存 / 另存为(docs/roadmap.md P0「文件操作」)。
//!
//! 读写都是 UTF-8 字节原样进出:不做 CRLF↔LF 转换、不补尾换行
//! (roadmap P0 验收「`.md` 文件保持原样(无格式化篡改)」)。对话框用 rfd
//! 同步版,在 `App::logic` 的归约里弹出——原生模态对话框本来就会阻塞
//! 事件循环,弹框期间本应用没有需要继续绘制的状态。
//!
//! 命令的 label 与快捷键统一收在 [`crate::command`] —— 单一事实源,不再
//! 分散;本模块只承载文件域的执行细节(对话框、读写、错误)。

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

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

/// 目录选择对话框:文件树根目录用(`ui::sidebar` 发消息,归约里弹出)。
/// 取消返回 `None`。起始目录必须存在,由调用方保证。
pub fn pick_folder_dialog(start_dir: &Path) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_directory(start_dir)
        .pick_folder()
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

/// UTF-8 写全文,字节原样落盘:不做换行符转换,不补尾换行。失败提示的
/// 动作名固定为「保存」;导出等其它写路径用 [`write_as`] 指定动作名。
pub fn write(path: &Path, text: &str) -> Result<(), FileError> {
    write_as("保存", path, text)
}

/// [`fn@write`] 的动作名可指定版本,失败提示形如「导出失败 路径: 原因」。
pub fn write_as(op: &'static str, path: &Path, text: &str) -> Result<(), FileError> {
    std::fs::write(path, text.as_bytes()).map_err(|source| err(op, path, source))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 进程内唯一且不冲突的临时路径;测试自删。
    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("latermd-file-{}-{name}", std::process::id()))
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
}
