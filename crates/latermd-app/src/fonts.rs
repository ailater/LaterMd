//! CJK 系统字体注入(M0 附加验证 5「中文渲染」的载体)。
//!
//! egui 内置字体只有拉丁字符,中文默认渲染为方块(tofu)。此处按候选路径表读
//! 系统字体,以回退形式追加到 Proportional / Monospace 两个族:拉丁字符仍走
//! 内置字体,CJK 落到系统字体。候选全失配时如实返回 `None`,界面显示警告,
//! 不静默吞掉。方案定案(不引入 fontdb / font-kit)见 docs/m0-report.md。

use std::sync::Arc;

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};

/// 候选 (字体文件, 比例字体 face index, 等宽字体 face index),按序取第一个存在的文件。
/// face index 是 .ttc 字体集合内的第 N 个字型,由 `fc-query` 枚举得出,只对
/// 对应文件有效;给候选表换文件时必须重查 index。
const CANDIDATES: &[(&str, u32, u32)] = &[
    // Deepin / 常见发行版的 noto 包:同一 .ttc 内含比例与等宽两套 SC 字型
    (
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        2,
        7,
    ),
    // 兜底:文泉驿微米黑,单字型集合,两个族共用 index 0
    ("/usr/share/fonts/truetype/wqy/wqy-microhei.ttc", 0, 0),
];

/// 注入 CJK 回退字体,返回加载来源描述(用户可见)。
pub fn install(ctx: &egui::Context) -> Option<String> {
    let &(path, prop_idx, mono_idx) = CANDIDATES
        .iter()
        .find(|(path, _, _)| std::path::Path::new(path).is_file())?;

    let bytes = std::fs::read(path).ok()?;
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
    Some(format!(
        "{path} (比例 face {prop_idx} / 等宽 face {mono_idx})"
    ))
}
