//! 字体：优先使用系统原生字体（Latin + CJK），失败时退回 egui 内置字体。

use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily};

fn load(path: &str, index: u32) -> Option<FontData> {
    let data = std::fs::read(path).ok()?;
    Some(FontData {
        font: Arc::new(data),
        index,
        tweak: Default::default(),
    })
}

pub fn install(ctx: &egui::Context) {
    let mut defs = FontDefinitions::default();

    #[cfg(target_os = "linux")]
    let (ui, bold, italic, bold_italic, cjk, ipa) = {
        let ui = load(
            "/usr/share/fonts/abattis-cantarell-fonts/Cantarell-Regular.otf",
            0,
        );
        let bold = load(
            "/usr/share/fonts/abattis-cantarell-fonts/Cantarell-Bold.otf",
            0,
        );
        let italic = load(
            "/usr/share/fonts/abattis-cantarell-fonts/Cantarell-Italic.otf",
            0,
        );
        let bold_italic = load(
            "/usr/share/fonts/abattis-cantarell-fonts/Cantarell-BoldItalic.otf",
            0,
        );
        let cjk = load(
            "/usr/share/fonts/google-droid-sans-fonts/DroidSansFallbackFull.ttf",
            0,
        );
        // Cantarell / DroidSansFallback 都缺 IPA 音标（ˈ ɛ ʃ 等），补 Latin 字体。
        let ipa = load(
            "/usr/share/fonts/liberation-sans-fonts/LiberationSans-Regular.ttf",
            0,
        )
        .or_else(|| load("/usr/share/fonts/gnu-free/FreeSans.ttf", 0))
        .or_else(|| {
            load(
                "/usr/share/fonts/google-carlito-fonts/Carlito-Regular.ttf",
                0,
            )
        });
        (ui, bold, italic, bold_italic, cjk, ipa)
    };
    #[cfg(target_os = "windows")]
    let (ui, bold, italic, bold_italic, cjk, ipa) = {
        let ui = load("C:\\Windows\\Fonts\\segoeui.ttf", 0);
        let bold = load("C:\\Windows\\Fonts\\segoeuib.ttf", 0);
        let italic = load("C:\\Windows\\Fonts\\segoeuii.ttf", 0);
        let bold_italic = load("C:\\Windows\\Fonts\\segoeuiz.ttf", 0);
        let cjk = load("C:\\Windows\\Fonts\\msyh.ttc", 0);
        // Segoe UI 的 IPA 覆盖不全时，用 Arial 兜底。
        let ipa = load("C:\\Windows\\Fonts\\arial.ttf", 0);
        (ui, bold, italic, bold_italic, cjk, ipa)
    };

    if let Some(f) = ui {
        defs.font_data.insert("ui".into(), Arc::new(f));
        defs.families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "ui".into());
    }
    let mut add_family = |name: &str, font: Option<FontData>, fallback: &str| {
        if let Some(f) = font {
            defs.font_data.insert(name.into(), Arc::new(f));
            defs.families
                .entry(FontFamily::Name(name.into()))
                .or_default()
                .push(name.into());
            if let Some(list) = defs.families.get_mut(&FontFamily::Name(name.into())) {
                list.push(fallback.into());
            }
        }
    };
    add_family("bold", bold, "ui");
    add_family("italic", italic, "ui");
    add_family("bold_italic", bold_italic, "ui");
    if let Some(f) = cjk {
        defs.font_data.insert("cjk".into(), Arc::new(f));
        for fam in [
            FontFamily::Proportional,
            FontFamily::Monospace,
            FontFamily::Name("bold".into()),
            FontFamily::Name("italic".into()),
            FontFamily::Name("bold_italic".into()),
        ] {
            defs.families.entry(fam).or_default().push("cjk".into());
        }
    }
    if let Some(f) = ipa {
        defs.font_data.insert("ipa".into(), Arc::new(f));
        for fam in [
            FontFamily::Proportional,
            FontFamily::Monospace,
            FontFamily::Name("bold".into()),
            FontFamily::Name("italic".into()),
            FontFamily::Name("bold_italic".into()),
        ] {
            let list = defs.families.entry(fam).or_default();
            // 放在 CJK 前面：CJK 字体通常没有 IPA，避免继续落到 .notdef 方框。
            let pos = list.iter().position(|n| n == "cjk").unwrap_or(list.len());
            list.insert(pos, "ipa".into());
        }
    }
    ctx.set_fonts(defs);
}

pub fn bold() -> FontFamily {
    FontFamily::Name("bold".into())
}
pub fn italic() -> FontFamily {
    FontFamily::Name("italic".into())
}
pub fn bold_italic() -> FontFamily {
    FontFamily::Name("bold_italic".into())
}
