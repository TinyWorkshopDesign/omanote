//! Screenshot-to-text, fully on device (no image ever leaves the machine):
//! * macOS / iOS: Apple Vision (`VNRecognizeTextRequest`);
//! * Linux: `tesseract`, which Omarchy ships (honours `OMARCHY_OCR_LANGS`);
//! * Android: not available yet.

#[allow(dead_code)]
pub const UNSUPPORTED: &str = "OCR_UNSUPPORTED";

#[cfg(any(target_os = "macos", target_os = "ios"))]
pub fn recognize(image: &[u8]) -> Result<String, String> {
    use objc2::AnyThread;
    use objc2_foundation::{NSArray, NSData, NSDictionary};
    use objc2_vision::{
        VNImageRequestHandler, VNRecognizeTextRequest, VNRequest, VNRequestTextRecognitionLevel,
    };

    objc2::rc::autoreleasepool(|_| {
        let data = NSData::with_bytes(image);
        let handler = VNImageRequestHandler::initWithData_options(
            VNImageRequestHandler::alloc(),
            &data,
            &NSDictionary::new(),
        );
        let request = VNRecognizeTextRequest::new();
        request.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);
        request.setUsesLanguageCorrection(true);
        request.setAutomaticallyDetectsLanguage(true);

        let as_request: &VNRequest = &request;
        let requests = NSArray::from_slice(&[as_request]);
        handler
            .performRequests_error(&requests)
            .map_err(|e| format!("OCR_FAILED|{}", e.localizedDescription()))?;

        let mut lines = Vec::new();
        if let Some(results) = request.results() {
            for obs in results.iter() {
                if let Some(best) = obs.topCandidates(1).firstObject() {
                    lines.push(best.string().to_string());
                }
            }
        }
        Ok(lines.join("\n"))
    })
}

#[cfg(target_os = "linux")]
pub fn recognize(image: &[u8]) -> Result<String, String> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let langs = tesseract_langs()?;
    let mut child = Command::new("tesseract")
        .args(["stdin", "stdout", "--oem", "1", "--psm", "6", "-l", &langs])
        .args(["-c", "preserve_interword_spaces=1"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "OCR_NO_TESSERACT".to_string())?;
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(image)
        .map_err(|e| format!("OCR_FAILED|{e}"))?;
    let out = child.wait_with_output().map_err(|e| format!("OCR_FAILED|{e}"))?;
    if !out.status.success() {
        return Err("OCR_FAILED|tesseract".into());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
}

/// `OMARCHY_OCR_LANGS` if set, otherwise every installed language among the
/// ones Omanote's UI speaks (English always first).
#[cfg(target_os = "linux")]
fn tesseract_langs() -> Result<String, String> {
    if let Ok(l) = std::env::var("OMARCHY_OCR_LANGS") {
        if !l.trim().is_empty() {
            return Ok(l);
        }
    }
    let out = std::process::Command::new("tesseract")
        .arg("--list-langs")
        .output()
        .map_err(|_| "OCR_NO_TESSERACT".to_string())?;
    let installed = String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    let wanted = ["eng", "ita", "deu", "fra", "spa", "por", "nld", "pol"];
    let langs: Vec<&str> = wanted
        .iter()
        .copied()
        .filter(|l| installed.lines().any(|x| x.trim() == *l))
        .collect();
    Ok(if langs.is_empty() { "eng".into() } else { langs.join("+") })
}

#[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "linux")))]
pub fn recognize(_image: &[u8]) -> Result<String, String> {
    Err(UNSUPPORTED.into())
}

/// Lets the user select a screen region and returns it as PNG bytes.
/// `Ok(None)` when the selection was cancelled.
#[cfg(target_os = "macos")]
pub fn capture_region() -> Result<Option<Vec<u8>>, String> {
    let path = std::env::temp_dir().join(format!("omanote-capture-{}.png", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let status = std::process::Command::new("screencapture")
        .args(["-i", "-x"])
        .arg(&path)
        .status()
        .map_err(|e| format!("OCR_FAILED|{e}"))?;
    if !status.success() || !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path).map_err(|e| format!("OCR_FAILED|{e}"))?;
    let _ = std::fs::remove_file(&path);
    Ok(Some(bytes))
}

/// Wayland (Omarchy): `slurp` to pick the region, `grim` to grab it.
#[cfg(target_os = "linux")]
pub fn capture_region() -> Result<Option<Vec<u8>>, String> {
    use std::process::Command;
    let sel = Command::new("slurp").output().map_err(|_| "OCR_NO_GRIM".to_string())?;
    let region = String::from_utf8_lossy(&sel.stdout).trim().to_string();
    if !sel.status.success() || region.is_empty() {
        return Ok(None);
    }
    let shot = Command::new("grim")
        .args(["-g", &region, "-"])
        .output()
        .map_err(|_| "OCR_NO_GRIM".to_string())?;
    if !shot.status.success() {
        return Err("OCR_FAILED|grim".into());
    }
    Ok(Some(shot.stdout))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn capture_region() -> Result<Option<Vec<u8>>, String> {
    Err(UNSUPPORTED.into())
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    #[test]
    fn reads_text_with_apple_vision() {
        let png = include_bytes!("../fixtures/ocr-sample.png");
        let t = std::time::Instant::now();
        let text = super::recognize(png).unwrap();
        eprintln!("first OCR: {:?}", t.elapsed());
        let t = std::time::Instant::now();
        super::recognize(png).unwrap();
        eprintln!("second OCR: {:?}", t.elapsed());
        assert!(text.contains("Lista della spesa"), "{text}");
        assert!(text.contains("42,50"), "{text}");
    }
}
