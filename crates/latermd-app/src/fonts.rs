//! CJK 系统字体注入(M0 附加验证 5「中文渲染」的载体)。
//!
//! egui 内置字体只有拉丁字符,中文默认渲染为方块(tofu)。此处按候选路径表读
//! 系统字体,以回退形式追加到 Proportional / Monospace 两个族:拉丁字符仍走
//! 内置字体,CJK 落到系统字体。候选全失配时如实返回 `None`,界面显示警告,
//! 不静默吞掉。方案定案(不引入 fontdb / font-kit)见 docs/m0-report.md。

use std::sync::Arc;

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};

/// 候选 (字体文件, 比例字体 face index, 等宽字体 face index),按序取第一个存在的文件。
/// face index 是 .ttc 字体集合内的第 N 个字型,Linux 两条由 `fc-query` 枚举得出;
/// Windows/macOS 条目为资料建议值,待真机核验(见 docs/m0-report.md 附加验证 5)。
/// 三平台路径前缀互斥,`is_file` 探测天然分流,不需要 #[cfg] 分表。
const CANDIDATES: &[(&str, u32, u32)] = &[
    // Linux(Deepin / 常见发行版的 noto 包):同一 .ttc 内含比例与等宽两套 SC 字型
    (
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        2,
        7,
    ),
    // Linux 兜底:文泉驿微米黑,单字型集合,两个族共用 index 0
    ("/usr/share/fonts/truetype/wqy/wqy-microhei.ttc", 0, 0),
    // Windows 11:msyh.ttc 的 face 0 = 微软雅黑(face 1 为 UI 变体);simhei 单字型;
    // msyhbd 为粗体集合,仅作最末兜底
    ("C:\\Windows\\Fonts\\msyh.ttc", 0, 0),
    ("C:\\Windows\\Fonts\\simhei.ttf", 0, 0),
    ("C:\\Windows\\Fonts\\msyhbd.ttc", 0, 0),
    // macOS 14:PingFang 的 index 0 为占位(任一 face 均含 CJK 可消除方块,
    // SC Regular 确切 index 待真机枚举后修正);Hiragino Sans GB 为简体兜底
    ("/System/Library/Fonts/PingFang.ttc", 0, 0),
    ("/System/Library/Fonts/Hiragino Sans GB.ttc", 0, 0),
];

/// 注入 CJK 回退字体,返回加载来源描述(用户可见)。
pub fn install(ctx: &egui::Context) -> Option<String> {
    for &(path, prop_idx, mono_idx) in CANDIDATES {
        if !std::path::Path::new(path).is_file() {
            continue;
        }
        // 读取失败(权限/竞态删除)跳到下一候选、不 panic;FontData::from_owned 只持有字节不做解析,.ttc 真正的解码在 set_fonts 之后的首帧、此处无从捕获
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let face = |index: u32| FontData {
            index,
            ..FontData::from_owned(bytes.clone())
        };

        let proportional = "latermd-cjk-proportional";
        let monospace = "latermd-cjk-monospace";
        let mut defs = FontDefinitions::default();
        defs.font_data
            .insert(proportional.into(), Arc::new(face(prop_idx)));
        defs.font_data
            .insert(monospace.into(), Arc::new(face(mono_idx)));
        for (family, name) in [
            (FontFamily::Proportional, proportional),
            (FontFamily::Monospace, monospace),
        ] {
            // push 到末尾 = 回退:拉丁字符命中内置字体后不再往下走
            defs.families.entry(family).or_default().push(name.into());
        }
        ctx.set_fonts(defs);
        return Some(format!(
            "{path} (比例 face {prop_idx} / 等宽 face {mono_idx})"
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::CANDIDATES;
    use std::collections::HashSet;

    /// `Path::is_absolute` 按宿主平台判定(Linux 上 `C:\...` 是相对路径),
    /// 故按目标平台语义判前缀:Unix 以 `/` 开头,Windows 为盘符路径。
    fn is_absolute_on_target_platform(path: &str) -> bool {
        path.starts_with('/') || path.as_bytes().get(1) == Some(&b':')
    }

    #[test]
    fn all_three_platforms_have_candidates() {
        let windows = CANDIDATES
            .iter()
            .filter(|(p, _, _)| p.starts_with("C:"))
            .count();
        let macos = CANDIDATES
            .iter()
            .filter(|(p, _, _)| p.starts_with("/System/"))
            .count();
        let linux = CANDIDATES
            .iter()
            .filter(|(p, _, _)| p.starts_with('/') && !p.starts_with("/System/"))
            .count();
        assert!(windows > 0, "Windows 候选为空");
        assert!(macos > 0, "macOS 候选为空");
        assert!(linux > 0, "Linux 候选为空");
    }

    #[test]
    fn candidate_paths_are_absolute_and_unique() {
        for (path, _, _) in CANDIDATES {
            assert!(is_absolute_on_target_platform(path), "非绝对路径: {path}");
        }
        let uniq: HashSet<&str> = CANDIDATES.iter().map(|(p, _, _)| *p).collect();
        assert_eq!(uniq.len(), CANDIDATES.len(), "候选路径存在重复");
    }

    #[test]
    fn windows_candidates_use_regular_face_zero() {
        // msyh / simhei / msyhbd 的 face 0 都是常规字型(1 为 UI/粗体变体);
        // 与 noto 不同,换 Windows 候选文件时沿用 0 前先重查
        for (path, prop, mono) in CANDIDATES {
            if path.starts_with("C:") {
                assert_eq!((*prop, *mono), (0, 0), "{} 偏离 face 0 约定", path);
            }
        }
    }
}
