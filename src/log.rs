use ic_canister_log::{declare_log_buffer, export as export_logs};

declare_log_buffer!(name = LOG, capacity = 1000);
declare_log_buffer!(name = ERR, capacity = 1000);

pub fn get_logs() -> Vec<String> {
    let mut result = vec![];
    for entry in export_logs(&LOG) {
        result.push(format!("{} {}", entry.timestamp, entry.message));
    }

    result
}

pub fn get_errors() -> Vec<String> {
    let mut result = vec![];
    for entry in export_logs(&ERR) {
        result.push(format!(
            "{}:{} {} {}",
            entry.file, entry.line, entry.message, entry.timestamp
        ));
    }

    result
}
