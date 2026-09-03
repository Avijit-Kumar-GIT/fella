//! Extract plain text from a PDF or text file, for `grep_files`/`read_file`.

use std::io::Read;

use crate::engine::error::{EngineError, EngineResult};

/// Read up to `max_bytes` of a text file, truncated at a UTF-8 boundary.
pub fn read_text_head(path: &str, max_bytes: usize) -> EngineResult<String> {
    let f = std::fs::File::open(path).map_err(|e| EngineError::io(format!("read {path}"), e))?;
    let mut buf = Vec::new();
    f.take(max_bytes as u64)
        .read_to_end(&mut buf)
        .map_err(|e| EngineError::io(format!("read {path}"), e))?;
    Ok(match String::from_utf8(buf) {
        Ok(s) => s,
        Err(e) => {
            let valid = e.utf8_error().valid_up_to();
            String::from_utf8_lossy(&e.into_bytes()[..valid]).into_owned()
        }
    })
}

/// Whole-file plain text of a PDF. Slow and unstreamable; callers cache it.
#[cfg(feature = "pdf")]
pub fn extract_pdf(path: &str) -> EngineResult<String> {
    let p = path.to_string();
    // `pdf_extract` has historically panicked on malformed input. Catch it so one
    // bad PDF fails this call instead of taking down the `ask` run. NB: the
    // shipped release profile is `panic = "abort"`, so this only saves us in
    // dev / test builds; a fuller fix would run the parse in a subprocess.
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        pdf_extract::extract_text(&p)
    }));
    match r {
        Ok(Ok(text)) => Ok(text),
        Ok(Err(e)) => Err(EngineError::msg(format!("could not read PDF {path}: {e}"))),
        Err(_) => Err(EngineError::msg(format!(
            "could not read {path}: the PDF is malformed"
        ))),
    }
}

#[cfg(not(feature = "pdf"))]
pub fn extract_pdf(path: &str) -> EngineResult<String> {
    let _ = path;
    Err(EngineError::msg(
        "this build has no PDF support (rebuild with `--features pdf`)",
    ))
}

/// A PDF that parses but yields almost no text is a scan (image-only pages).
/// Worth flagging so the model doesn't treat an empty read as "nothing here".
pub fn looks_like_no_text_layer(text: &str) -> bool {
    text.split_whitespace().take(6).count() < 6
}

/// Scan a text file line by line without holding the whole file in memory,
/// calling `f(line_number, line)` for each. Stops early when `f` returns `false`.
pub fn grep_lines(
    path: &str,
    mut f: impl FnMut(usize, &str) -> bool,
) -> EngineResult<()> {
    use std::io::BufRead;
    let file = std::fs::File::open(path).map_err(|e| EngineError::io(format!("read {path}"), e))?;
    let mut reader = std::io::BufReader::new(file);
    let mut line = Vec::new();
    let mut n = 0usize;
    loop {
        line.clear();
        let read = reader
            .read_until(b'\n', &mut line)
            .map_err(|e| EngineError::io(format!("read {path}"), e))?;
        if read == 0 {
            break;
        }
        n += 1;
        let text = String::from_utf8_lossy(&line);
        if !f(n, text.trim_end_matches(['\n', '\r'])) {
            break;
        }
    }
    Ok(())
}
