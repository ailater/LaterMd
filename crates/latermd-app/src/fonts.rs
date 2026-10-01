//! 字体接入:M0 CJK 系统回退 + U1 Inter 三字重(docs/ui-modernization.md §3 U1)。
//!
//! egui 内置字体只有拉丁字符,中文默认渲染为方块(tofu)。此处叠加两层:
//!
//! - **Inter(嵌入资源)**:Regular 挂 Proportional **首位**,拉丁字形优先
//!   命中 Inter;Medium/SemiBold 注册为独立 `FontFamily::Name` 供工具条/
//!   标题按字重取用(egui 无字重轴,权重即族名)。include_bytes 随包分发,
//!   SIL OFL 1.1,许可文本在 assets/fonts/LICENSE-Inter.txt。
//! - **CJK 系统回退**:按候选路径表读系统字体,追加到 Proportional /
//!   Monospace、两个 Inter 权重族与 `bold` 别名族(预览标题/加粗)的链尾。
//!   候选全失配时如实返回 `None`,
//!   界面显示警告,不静默吞掉;Inter 照装(嵌入资源不依赖系统)。
//!   不引 fontdb / font-kit 的定案见 docs/m0-report.md。
//!
//! egui 出厂链(Ubuntu-Light → NotoEmoji → emoji-icon-font)的**相对顺序
//! 不动**:Inter 没有 emoji 字形,NotoEmoji 仍是 emoji 的第一个命中;把
//! CJK 或 Inter 插到它前面会让 emoji 落到 CJK 字体或变豆腐块。

use std::path::Path;
use std::sync::Arc;

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};

static INTER_REGULAR: &[u8] = include_bytes!("../../../assets/fonts/Inter-Regular.ttf");
static INTER_MEDIUM: &[u8] = include_bytes!("../../../assets/fonts/Inter-Medium.ttf");
static INTER_SEMIBOLD: &[u8] = include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf");

/// Inter Regular 的 font_data 键,同时是 Proportional 链首名。
const NAME_REGULAR: &str = "Inter-Regular";
/// Inter SemiBold 的族名:工具条 / 标题等强调位用(正文 Regular)。
pub const FAMILY_SEMIBOLD: &str = "Inter-SemiBold";
/// vendored `egui_markdown` 探测的字面量族名:注册后表头/加粗走 SemiBold
/// 字重而非 `strong_text_color` 回落(后者 = active 控件前景,投影后是 accent
/// 蓝,会把 `**加粗**` 染成链接色;decisions-pending #48)。
pub const FAMILY_BOLD: &str = "bold";
/// Inter Medium 的族名:介于正文与强调之间,U1 注册备用、暂无消费者。
pub const FAMILY_MEDIUM: &str = "Inter-Medium";

/// CJK 回退在 font_data 里的两个键(比例 / 等宽 face)。
const CJK_PROPORTIONAL: &str = "latermd-cjk-proportional";
const CJK_MONOSPACE: &str = "latermd-cjk-monospace";

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

/// 注入字体,返回 CJK 回退的加载来源(用户可见);`None` = 候选全失配,
/// 中文将显示为方块 —— 但 Inter 三字重照常生效(嵌入资源,与系统无关)。
pub fn install(ctx: &egui::Context) -> Option<String> {
    for &(path, prop_idx, mono_idx) in CANDIDATES {
        if !Path::new(path).is_file() {
            continue;
        }
        // 读取失败(权限/竞态删除)跳到下一候选、不 panic;FontData::from_owned 只持有字节不做解析,.ttc 真正的解码在 set_fonts 之后的首帧、此处无从捕获
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        ctx.set_fonts(build_definitions(Some((&bytes, prop_idx, mono_idx))));
        mark_installed(ctx);
        return Some(format!(
            "{path} (比例 face {prop_idx} / 等宽 face {mono_idx})"
        ));
    }
    ctx.set_fonts(build_definitions(None));
    mark_installed(ctx);
    None
}

/// 构建字体定义:Inter 三字重 + 可选 CJK 回退。纯函数、不触 context ——
/// 链顺序单测由此注入假字节(字体解析发生在 set_fonts 之后的首帧,此处
/// 只排链)。
fn build_definitions(cjk: Option<(&[u8], u32, u32)>) -> FontDefinitions {
    let mut defs = FontDefinitions::default();
    for (name, bytes) in [
        (NAME_REGULAR, INTER_REGULAR),
        (FAMILY_MEDIUM, INTER_MEDIUM),
        (FAMILY_SEMIBOLD, INTER_SEMIBOLD),
    ] {
        defs.font_data
            .insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    }
    // 出厂 Proportional 链(Ubuntu-Light → NotoEmoji → emoji-icon-font)整体
    // 后移一位:Inter 插首位,emoji 相对顺序不变。先取尾巴再插入。
    let fallback_tail: Vec<String> = defs.families[&FontFamily::Proportional].clone();
    defs.families
        .get_mut(&FontFamily::Proportional)
        .expect("出厂 Proportional 族恒存在")
        .insert(0, NAME_REGULAR.to_owned());
    // Medium/SemiBold 独立族:链头是对应字重,其余与 Proportional 同构 ——
    // emoji 链照旧,CJK 回退也挂尾(工具条/标题是中文,链尾无 CJK 会变方块)
    // `bold` 是 SemiBold 的别名族(vendored 渲染按字面量探测,decisions #48)
    for name in [FAMILY_MEDIUM, FAMILY_SEMIBOLD] {
        let mut chain = vec![name.to_owned()];
        chain.extend(fallback_tail.iter().cloned());
        defs.families
            .insert(FontFamily::Name(Arc::from(name)), chain);
    }
    let mut bold_chain = vec![FAMILY_SEMIBOLD.to_owned()];
    bold_chain.extend(fallback_tail.iter().cloned());
    defs.families
        .insert(FontFamily::Name(Arc::from(FAMILY_BOLD)), bold_chain);
    if let Some((bytes, prop_idx, mono_idx)) = cjk {
        let face = |index: u32| FontData {
            index,
            ..FontData::from_owned(bytes.to_vec())
        };
        defs.font_data
            .insert(CJK_PROPORTIONAL.to_owned(), Arc::new(face(prop_idx)));
        defs.font_data
            .insert(CJK_MONOSPACE.to_owned(), Arc::new(face(mono_idx)));
        for (family, name) in [
            (FontFamily::Proportional, CJK_PROPORTIONAL),
            (FontFamily::Monospace, CJK_MONOSPACE),
            (FontFamily::Name(Arc::from(FAMILY_MEDIUM)), CJK_PROPORTIONAL),
            (
                FontFamily::Name(Arc::from(FAMILY_SEMIBOLD)),
                CJK_PROPORTIONAL,
            ),
            // `bold` 别名族:预览标题(heading)与加粗文本经 vendored
            // apply_bold 切到它,链尾无 CJK 会整行变方块
            (FontFamily::Name(Arc::from(FAMILY_BOLD)), CJK_PROPORTIONAL),
        ] {
            // push 到末尾 = 回退:拉丁字符命中 Inter 后不再往下走
            defs.families
                .entry(family)
                .or_default()
                .push(name.to_owned());
        }
    }
    defs
}

/// SemiBold 族(工具条 / 标题取字重用)。未注册的 `FontFamily::Name` 在
/// epaint 里直接 panic(`Font::font` 无 fallback),而大量无头 UI 测试不走
/// main 的 install —— 探测不到安装标志就回落 Proportional,不炸测试。
pub fn semibold_family(ctx: &egui::Context) -> FontFamily {
    let installed = ctx.data(|data| data.get_temp::<bool>(installed_id()).unwrap_or(false));
    if installed {
        FontFamily::Name(Arc::from(FAMILY_SEMIBOLD))
    } else {
        FontFamily::Proportional
    }
}

fn installed_id() -> egui::Id {
    egui::Id::new("latermd-fonts-installed")
}

fn mark_installed(ctx: &egui::Context) {
    ctx.data_mut(|data| data.insert_temp(installed_id(), true));
}

/// 测试取证用:#43 M1 混排复现需要直接解析命中的 CJK 回退字体文件
/// (路径 + 两个 face index),与 `install` 同一条候选探测路径。
/// 未命中任何候选时返回 `None`(本机无 CJK 字体,如实跳过)。
#[cfg(test)]
pub(crate) fn cjk_source_for_test() -> Option<(&'static str, u32, u32)> {
    CANDIDATES
        .iter()
        .copied()
        .find(|(path, _, _)| Path::new(path).is_file())
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

    /// U1 验收:字体链顺序。Inter 恒在 Proportional 首位;出厂链
    /// (Ubuntu-Light → NotoEmoji → emoji-icon-font)相对顺序不变 ——
    /// NotoEmoji 仍是 emoji 的第一命中,不得被 Inter 或 CJK 挤后;CJK 回退
    /// 挂在四个族的链尾。假字节不进 set_fonts,不触发解析。
    #[test]
    fn inter_leads_and_emoji_order_survives() {
        let fake = [0u8; 16];
        let defs = build_definitions(Some((&fake, 2, 7)));
        let chain = |family: &FontFamily| -> &[String] {
            defs.families
                .get(family)
                .unwrap_or_else(|| panic!("{family:?} 未注册"))
        };
        let pos = |names: &[String], name: &str| {
            names
                .iter()
                .position(|n| n == name)
                .unwrap_or_else(|| panic!("{name} 不在链里: {names:?}"))
        };

        let prop = chain(&FontFamily::Proportional);
        assert_eq!(prop.first().map(String::as_str), Some(NAME_REGULAR));
        assert!(pos(prop, NAME_REGULAR) < pos(prop, "Ubuntu-Light"));
        assert!(pos(prop, "Ubuntu-Light") < pos(prop, "NotoEmoji-Regular"));
        assert!(pos(prop, "NotoEmoji-Regular") < pos(prop, "emoji-icon-font"));
        assert_eq!(prop.last().map(String::as_str), Some(CJK_PROPORTIONAL));

        // Inter 非等宽,不进 Monospace;CJK 照挂尾
        let mono = chain(&FontFamily::Monospace);
        assert!(!mono.iter().any(|n| n == NAME_REGULAR));
        assert_eq!(mono.last().map(String::as_str), Some(CJK_MONOSPACE));

        // 三字重数据齐全;权重族链头是对应字重,emoji 链与 CJK 回退同构
        for name in [NAME_REGULAR, FAMILY_MEDIUM, FAMILY_SEMIBOLD] {
            assert!(defs.font_data.contains_key(name), "{name} 缺字体数据");
        }
        for (family, head) in [
            (FontFamily::Name(Arc::from(FAMILY_MEDIUM)), FAMILY_MEDIUM),
            (
                FontFamily::Name(Arc::from(FAMILY_SEMIBOLD)),
                FAMILY_SEMIBOLD,
            ),
            // bold 别名族链头也是 SemiBold,且必须吃到 CJK 回退(预览标题中文)
            (FontFamily::Name(Arc::from(FAMILY_BOLD)), FAMILY_SEMIBOLD),
        ] {
            let weight = chain(&family);
            assert_eq!(weight.first().map(String::as_str), Some(head));
            assert!(pos(weight, head) < pos(weight, "NotoEmoji-Regular"));
            assert!(pos(weight, "NotoEmoji-Regular") < pos(weight, "emoji-icon-font"));
            assert_eq!(weight.last().map(String::as_str), Some(CJK_PROPORTIONAL));
        }
    }

    /// 候选全失配(`None`):Inter 仍注册(嵌入资源),四族链上无 CJK 条目。
    #[test]
    fn inter_registers_even_without_cjk() {
        let defs = build_definitions(None);
        assert_eq!(
            defs.families[&FontFamily::Proportional]
                .first()
                .map(String::as_str),
            Some(NAME_REGULAR)
        );
        for family in [
            FontFamily::Proportional,
            FontFamily::Monospace,
            FontFamily::Name(Arc::from(FAMILY_MEDIUM)),
            FontFamily::Name(Arc::from(FAMILY_SEMIBOLD)),
            FontFamily::Name(Arc::from(FAMILY_BOLD)),
        ] {
            assert!(
                !defs.families[&family].iter().any(|n| n == CJK_PROPORTIONAL),
                "{family:?} 不应残留 CJK 条目"
            );
        }
    }

    /// install 端到端:无论本机有无 CJK 候选,context 里 Proportional 首位
    /// 恒为 Inter;装完 `semibold_family` 不再回落。
    #[test]
    fn install_puts_inter_first_in_context() {
        let ctx = egui::Context::default();
        let cjk = install(&ctx);
        ctx.run_ui(egui::RawInput::default(), |_| {})
            .drop_without_applying_deltas();
        ctx.fonts(|f| {
            let prop = &f.definitions().families[&FontFamily::Proportional];
            assert_eq!(prop.first().map(String::as_str), Some(NAME_REGULAR));
            match cjk {
                Some(_) => assert_eq!(prop.last().map(String::as_str), Some(CJK_PROPORTIONAL)),
                None => assert!(prop.iter().all(|n| n != CJK_PROPORTIONAL)),
            }
        });
        assert_eq!(
            semibold_family(&ctx),
            FontFamily::Name(Arc::from(FAMILY_SEMIBOLD))
        );
    }

    /// U1 验收:Inter 与 CJK 混排的基线/行高对齐(无头 galley 布局对比)。
    /// egui 的行 metrics 取**链头**字体(此处 Inter),fallback 字形(Noto
    /// CJK)共用同一基线与行框 —— 断言纯拉丁 / 纯 CJK / 混排三条 galley
    /// 单行且 rect 高度一致,即混排不分行、行高不跳变。CJK 侧断言依赖本机
    /// 候选命中(CI runner 可能无 CJK 字体,如实跳过);三平台真机目视
    /// 验收待人工(docs/ui-modernization.md §2.2)。
    #[test]
    fn mixed_script_shares_row_height_and_baseline() {
        let ctx = egui::Context::default();
        let has_cjk = install(&ctx).is_some();
        ctx.run_ui(egui::RawInput::default(), |_| {})
            .drop_without_applying_deltas();
        let font = egui::FontId::new(14.0, FontFamily::Proportional);
        let layout = |text: &str| {
            ctx.fonts_mut(|f| f.layout_no_wrap(text.to_owned(), font.clone(), egui::Color32::WHITE))
        };

        let latin = layout("LaterMD Inter");
        let mixed = layout("LaterMD 中文混排 Inter");
        for (name, galley) in [("latin", &latin), ("mixed", &mixed)] {
            assert_eq!(galley.rows.len(), 1, "{name}: 不换行(宽度无限)");
            assert!(
                galley.rect.height().is_finite() && galley.rect.height() > 0.0,
                "{name}: 行高无效 {}",
                galley.rect.height()
            );
        }
        assert_eq!(
            latin.rect.height(),
            mixed.rect.height(),
            "混排不得改变行高(fallback 字形共用链头 metrics)"
        );

        if !has_cjk {
            eprintln!("本机无 CJK 候选字体,CJK 侧断言跳过");
            return;
        }
        let cjk = layout("中文字体测试");
        assert_eq!(
            cjk.rect.height(),
            latin.rect.height(),
            "CJK 也吃链头 Inter 的行 metrics"
        );
        assert!(
            ctx.fonts_mut(|f| f.has_glyphs(&font, "中文混排")),
            "CJK 回退链生效,不出豆腐块"
        );
        // 权重族也要能显示中文:工具条/标题是中文,链尾无 CJK 会变方块
        let semibold = egui::FontId::new(14.0, semibold_family(&ctx));
        assert!(
            ctx.fonts_mut(|f| f.has_glyphs(&semibold, "标题")),
            "SemiBold 族的 CJK 回退链生效"
        );
        // bold 别名族:预览 heading/加粗经 vendored apply_bold 切到它,
        // 漏挂 CJK 曾让标题中文整行变方块
        let bold = egui::FontId::new(14.0, FontFamily::Name(Arc::from(FAMILY_BOLD)));
        assert!(
            ctx.fonts_mut(|f| f.has_glyphs(&bold, "标题加粗")),
            "bold 别名族的 CJK 回退链生效"
        );
    }
}
