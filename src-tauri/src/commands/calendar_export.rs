use std::path::PathBuf;
use tauri::ipc::Response;

fn system_font_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    #[cfg(target_os = "windows")]
    {
        let fonts =
            PathBuf::from(std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into()))
                .join("Fonts");
        for name in [
            "NotoSansTC-VF.ttf",
            "NotoSansHK-VF.ttf",
            "msjh.ttc",
            "msjhl.ttc",
            "kaiu.ttf",
            "mingliu.ttc",
            "simsunb.ttf",
            "SimsunExtG.ttf",
        ] {
            candidates.push(fonts.join(name));
        }
    }

    #[cfg(target_os = "macos")]
    {
        for path in [
            "/System/Library/Fonts/PingFang.ttc",
            "/System/Library/Fonts/Supplemental/Songti.ttc",
            "/Library/Fonts/Arial Unicode.ttf",
        ] {
            candidates.push(PathBuf::from(path));
        }
    }

    #[cfg(all(unix, not(target_os = "macos"), not(target_os = "android")))]
    {
        for path in [
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJKtc-Regular.otf",
            "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
            "/usr/share/fonts/truetype/unifont/unifont.ttf",
        ] {
            candidates.push(PathBuf::from(path));
        }
    }

    candidates
}

#[tauri::command]
pub fn get_calendar_pdf_font() -> Result<Response, String> {
    for path in system_font_candidates() {
        if !path.is_file() {
            continue;
        }
        return std::fs::read(&path)
            .map(Response::new)
            .map_err(|error| format!("Couldn't read the system PDF font: {error}"));
    }

    Err("No compatible CJK system font was found. Install Noto Sans CJK to export non-English text to PDF.".into())
}
