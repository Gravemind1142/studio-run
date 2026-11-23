//! Binary inputs (`--input`) and output (`--binary-output`), shared by both hosts.
//!
//! Locally, inputs are embedded in the wrapper as string literals and the
//! output comes back base64-encoded in `binary` events. Under `--cloud`, the
//! inputs are packed into the task's one binary input, and the output follows
//! the events in the task's binary output. See docs/design.md.

use std::path::Path;

/// Total size of the inputs, packed as for `--cloud`. Open Cloud's limit on a
/// binary input; the local host uses the same one.
pub const MAX_INPUT_BYTES: usize = 100 * 1024 * 1024;

/// Magic at the start of packed inputs (`pack`).
const INPUTS_MAGIC: &[u8; 4] = b"SRI1";
/// Magic at the start of a cloud task's binary output (`split_task_output`).
const OUTPUT_MAGIC: &[u8; 4] = b"SRB1";

#[derive(Debug)]
pub struct Input {
    pub name: String,
    pub data: Vec<u8>,
}

/// Reads `--input name=<file>` arguments.
pub fn read_inputs(specs: &[String]) -> Result<Vec<Input>, String> {
    let mut inputs: Vec<Input> = Vec::with_capacity(specs.len());
    for spec in specs {
        let Some((name, file)) = spec.split_once('=') else {
            return Err(format!("--input {spec}: expected NAME=FILE"));
        };
        if name.is_empty() || file.is_empty() {
            return Err(format!("--input {spec}: expected NAME=FILE"));
        }
        if inputs.iter().any(|i| i.name == name) {
            return Err(format!("--input {name} is given more than once"));
        }
        let data = std::fs::read(file).map_err(|e| format!("couldn't read {file}: {e}"))?;
        inputs.push(Input {
            name: name.to_string(),
            data,
        });
        if packed_len(&inputs) > MAX_INPUT_BYTES {
            return Err(format!(
                "the inputs are too large: at most {} MiB in total",
                MAX_INPUT_BYTES / 1024 / 1024
            ));
        }
    }
    Ok(inputs)
}

/// The size of `pack(inputs)`.
pub fn packed_len(inputs: &[Input]) -> usize {
    8 + inputs
        .iter()
        .map(|i| 8 + i.name.len() + i.data.len())
        .sum::<usize>()
}

/// Packs inputs into one buffer for a cloud task's binary input, which the
/// cloud wrapper unpacks: `SRI1`, the count, then for each input its name's
/// length, name, data's length and data. Lengths are little-endian u32s.
pub fn pack(inputs: &[Input]) -> Vec<u8> {
    let mut out = Vec::with_capacity(packed_len(inputs));
    out.extend_from_slice(INPUTS_MAGIC);
    out.extend_from_slice(&(inputs.len() as u32).to_le_bytes());
    for i in inputs {
        out.extend_from_slice(&(i.name.len() as u32).to_le_bytes());
        out.extend_from_slice(i.name.as_bytes());
        out.extend_from_slice(&(i.data.len() as u32).to_le_bytes());
        out.extend_from_slice(&i.data);
    }
    out
}

/// Splits a cloud task's binary output into its event lines and the script's
/// binary output. The cloud wrapper writes `SRB1`, the length of the events,
/// the length of the binary output (little-endian u32s), then both. Anything
/// else is taken as event lines with no binary output.
pub fn split_task_output(bytes: &[u8]) -> (&[u8], Option<&[u8]>) {
    let header = bytes.strip_prefix(OUTPUT_MAGIC.as_slice());
    let Some(rest) = header.filter(|r| r.len() >= 8) else {
        return (bytes, None);
    };
    let events_len = u32::from_le_bytes(rest[0..4].try_into().unwrap()) as usize;
    let data_len = u32::from_le_bytes(rest[4..8].try_into().unwrap()) as usize;
    let body = &rest[8..];
    if events_len.checked_add(data_len) != Some(body.len()) {
        return (bytes, None);
    }
    let (events, data) = body.split_at(events_len);
    (events, Some(data))
}

/// Decodes the base64 (URL-safe alphabet, optional padding) in `binary` events.
pub fn base64_decode(text: &str) -> Result<Vec<u8>, String> {
    let text = text.trim_end_matches('=').as_bytes();
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let mut acc: u32 = 0;
    let mut bits = 0;
    for &c in text {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return Err(format!("invalid base64 character {:?}", c as char)),
        };
        acc = (acc << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Ok(out)
}

/// Writes the script's binary output to `--binary-output`.
pub fn write_output(path: &Path, data: &[u8]) -> Result<(), String> {
    std::fs::write(path, data).map_err(|e| format!("couldn't write {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(name: &str, data: &[u8]) -> Input {
        Input {
            name: name.into(),
            data: data.to_vec(),
        }
    }

    #[test]
    fn packs_inputs_with_an_index() {
        let packed = pack(&[input("a", b"xyz"), input("bc", b"")]);
        assert_eq!(
            packed,
            b"SRI1\x02\0\0\0\x01\0\0\0a\x03\0\0\0xyz\x02\0\0\0bc\0\0\0\0"
        );
        assert_eq!(
            packed.len(),
            packed_len(&[input("a", b"xyz"), input("bc", b"")])
        );
        assert_eq!(pack(&[]), b"SRI1\0\0\0\0");
    }

    #[test]
    fn splits_task_output() {
        let out = b"SRB1\x05\0\0\0\x02\0\0\0lines\xff\x00";
        assert_eq!(
            split_task_output(out),
            (&b"lines"[..], Some(&b"\xff\x00"[..]))
        );
        // No header: all events.
        assert_eq!(split_task_output(b"@@x"), (&b"@@x"[..], None));
        // Lengths that don't add up: taken as events.
        let bad = b"SRB1\x05\0\0\0\x09\0\0\0lines";
        assert_eq!(split_task_output(bad), (&bad[..], None));
    }

    #[test]
    fn decodes_base64() {
        assert_eq!(base64_decode("").unwrap(), b"");
        assert_eq!(base64_decode("Zg==").unwrap(), b"f");
        assert_eq!(base64_decode("Zm8").unwrap(), b"fo");
        assert_eq!(base64_decode("Zm9v").unwrap(), b"foo");
        assert_eq!(base64_decode("_-8A").unwrap(), b"\xff\xef\x00");
        assert!(base64_decode("a+b/").is_err());
    }

    #[test]
    fn rejects_bad_input_specs() {
        assert!(read_inputs(&["noequals".into()]).is_err());
        assert!(read_inputs(&["=file".into()]).is_err());
        assert!(read_inputs(&["name=".into()]).is_err());
        let file = std::env::temp_dir().join("studio-run-binary-test-input");
        std::fs::write(&file, b"hi").unwrap();
        let spec = format!("x={}", file.display());
        let inputs = read_inputs(std::slice::from_ref(&spec)).unwrap();
        assert_eq!(inputs[0].name, "x");
        assert_eq!(inputs[0].data, b"hi");
        assert!(
            read_inputs(&[spec.clone(), spec])
                .unwrap_err()
                .contains("more than once")
        );
        let _ = std::fs::remove_file(file);
    }
}
