//! UI 子模块。绘制都在这里;状态归约只在 `App::logic`(铁律,docs/adr-005 §2.3)。

pub mod editor;
pub mod emoji_data;
pub mod emoji_panel;
pub mod fade;
pub mod format_bar;
pub mod gutter;
pub mod icons;
pub mod image_dialog;
pub mod layout;
pub mod menubar;
pub mod preview;
pub mod sidebar;
pub mod tabs;
pub mod titlebar;
pub mod tokens;

/// #39 M1 切换卡顿取证 harness:纯测试模块,生产构建不编译。
#[cfg(test)]
mod tab_switch_perf;
