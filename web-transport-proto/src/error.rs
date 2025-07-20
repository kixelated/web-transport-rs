// WebTransport shares with HTTP/3, so we can't start at 0 or use the full VarInt.
const ERROR_FIRST: u64 = 0x52e4a40fa8db;
const ERROR_LAST: u64 = 0x52e4a40fa9e2;

pub fn error_from_http3(code: u64) -> Option<u32> {
    if !(ERROR_FIRST..=ERROR_LAST).contains(&code) {
        return None;
    }

    // Check for reserved code points: (h - 0x21) % 0x1f == 0
    if (code - 0x21) % 0x1f == 0 {
        return None;
    }

    let shifted = code - ERROR_FIRST;
    let code = shifted - shifted / 0x1f;

    Some(code.try_into().unwrap())
}

pub fn error_to_http3(code: u32) -> u64 {
    ERROR_FIRST + code as u64 + code as u64 / 0x1e
}
