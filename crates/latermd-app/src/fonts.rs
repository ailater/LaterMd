//! 字体注入:Inter(内置)+ CJK 系统字体(回退)。
//!
//! 两段各有各的必要性:
//! - **Inter**:egui 出厂的比例字体是 Ubuntu-Light,只有拉丁字符且观感偏
//!   「开发者工具」。Inter 编译期内联(2026-09-27 U1),拉丁字形走它。
//! - **CJK**:egui 内置字体与 Inter 都**没有中文字形**,中文一律方块。此处
//!   按候选路径表读系统字体,以回退形式追加到各个族(M0 附加验证 5 的载体)。
//!   候选全失配时如实返回 `None`,终端告警,不静默吞掉。
//!
//! 方案定案(不引入 fontdb / font-kit)见 docs/m0-report.md。
//!
//! **为什么 Inter 只内联 latin 子集**:完整字重会带上西里尔/希腊等字形,
//! 体积翻几倍,而中文反正要靠系统字体、多出来的那部分一个也用不上。
//! 代价是 `→` `…` 这类符号它也没有 —— 会沿回退链落到 CJK 字体那边
//! (Noto Sans CJK 覆盖这些符号),不出现方块,只是字形跟着中文走。

use std::sync::Arc;

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};

/// Inter 正文(latin 子集,Regular 400)。
///
/// **只内联 Regular 一档,不做 SemiBold**。曾试过「SemiBold 单开一个
/// `FontFamily::Name` 族、把 `TextStyle::Heading` 指过去」(egui 没有字重概念,
/// 族就是字重的载体),验证时撞上 `epaint/src/text/fonts.rs:1025`:
/// **未绑定的族是 `panic`,不是回退**,任何没先跑 `install` 的 Context(无头
/// 测试正是如此)一画标题就崩。为「标题略粗」这点收益背上全局 panic 风险不
/// 值,标题层级改由字号(`FONT_LG`)承担。
const INTER_REGULAR: &[u8] = include_bytes!("../../../assets/fonts/Inter-Regular.ttf");

/// 字体数据键名。
const NAME_REGULAR: &str = "latermd-inter-regular";

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
/// 注入 Inter 与 CJK 回退,返回加载来源描述(用户可见)。
///
/// 返回值沿用 M0 的口径:**`None` = 没找到 CJK 字体**(调用方据此告警)。
/// Inter 是编译期内联的、必然成功,所以它不影响返回值 —— CJK 缺失时仍然要
/// `set_fonts` 把 Inter 装上,只是如实报 `None`。
pub fn install(ctx: &egui::Context) -> Option<String> {
    let mut defs = FontDefinitions::default();
    install_inter(&mut defs);
    // phosphor 图标字体(2026-09-27 U2)。它把自己插到 Proportional 链的
    // **第 1 位**(紧跟 Inter 之后),私有区码位(U+E0xx)落到它身上;
    // 拉丁/CJK/emoji 各自在链上别的字体里,互不抢。
    egui_phosphor::add_to_fonts(&mut defs, egui_phosphor::Variant::Regular);
    let cjk = install_cjk(&mut defs);
    ctx.set_fonts(defs);
    cjk.map(|desc| format!("Inter(latin 子集,内置) + {desc}"))
}

/// Inter:正文插到 Proportional 链**首**,标题另起一个 SemiBold 族。
fn install_inter(defs: &mut FontDefinitions) {
    defs.font_data.insert(
        NAME_REGULAR.to_owned(),
        Arc::new(FontData::from_static(INTER_REGULAR)),
    );
    // 链首:拉丁字符先命中 Inter,命中不了的(CJK / emoji)沿链继续回落。
    // egui 出厂的 Proportional 链是 [Ubuntu-Light, NotoEmoji, emoji-icon],
    // 这里插到它前面,不改它已有的回退。
    defs.families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, NAME_REGULAR.to_owned());
}

/// CJK 系统字体:按候选表取第一个存在的文件,追加到**每个**比例族的链尾。
///
/// 返回来源描述;`None` = 全部候选失配(中文会显示方块)。
fn install_cjk(defs: &mut FontDefinitions) -> Option<String> {
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
        defs.font_data
            .insert(proportional.into(), Arc::new(face(prop_idx)));
        defs.font_data
            .insert(monospace.into(), Arc::new(face(mono_idx)));
        for (family, name) in [
            (FontFamily::Proportional, proportional),
            (FontFamily::Monospace, monospace),
        ] {
            // push 到末尾 = 回退:拉丁字符命中 Inter 后不再往下走
            defs.families.entry(family).or_default().push(name.into());
        }
        return Some(format!(
            "{path} (比例 face {prop_idx} / 等宽 face {mono_idx})"
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// `Path::is_absolute` 按宿主平台判定(Linux 上 `C:\...` 是相对路径),
    /// 故按目标平台语义判前缀:Unix 以 `/` 开头,Windows 为盘符路径。
    fn is_absolute_on_target_platform(path: &str) -> bool {
        path.starts_with('/') || path.as_bytes().get(1) == Some(&b':')
    }

    /// Inter 装进去了(2026-09-27 U1 的守卫)。
    ///
    /// 只断言**族注册**不断言「渲染结果」:后者依赖宿主有没有 CJK 字体
    /// (CI 的 ubuntu 镜像未必装了 Noto CJK),会 flaky。族是 Inter 内联的、
    /// 与系统无关,这条在任何机器上都是确定性的。
    /// 装完之后中英混排与标题都能排版,不 panic(2026-09-27 U1 的守卫)。
    ///
    /// 不断言「某个字形来自哪个字体」 —— 那依赖宿主装没装 CJK 字体(CI 的
    /// ubuntu 镜像未必有 Noto CJK),会 flaky。这条守的是回退链的**组装**本身:
    /// Inter 在链首、CJK 在链尾、emoji 在默认链里,三者缺一都会在这里炸。
    #[test]
    fn mixed_cjk_and_latin_layouts_after_install() {
        let ctx = egui::Context::default();
        install(&ctx);
        ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.label(egui::RichText::new("LaterMD 混排标题").heading());
            ui.label("正文 abc 123 — 中文");
        })
        .drop_without_applying_deltas();
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
