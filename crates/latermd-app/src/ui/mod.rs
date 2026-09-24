//! UI 子模块。绘制都在这里;状态归约只在 `App::logic`(铁律,docs/adr-005 §2.3)。

pub mod editor;
pub mod layout;
pub mod menubar;
pub mod preview;
pub mod sidebar;
pub mod toolbar;
