use std::process::Command;

#[derive(Debug, Clone)]
pub struct ServiceInfo {
    pub name: String,
    pub load_state: String,
    pub active_state: String,
    pub sub_state: String,
    pub description: String,
}

impl ServiceInfo {
    pub fn status_display(&self) -> &str {
        match self.active_state.as_str() {
            "active" => match self.sub_state.as_str() {
                "running" => "running",
                "exited" => "exited",
                "waiting" => "waiting",
                "mounted" => "mounted",
                "listening" => "listening",
                _ => "active",
            },
            "inactive" => "inactive",
            "failed" => "failed",
            "activating" => "activating",
            "deactivating" => "deactivating",
            _ => "unknown",
        }
    }
}

#[derive(Debug, Clone)]
pub struct BootTime {
    pub kernel: String,
    pub userspace: String,
    pub total: String,
    pub firmware: Option<String>,
    pub loader: Option<String>,
    pub raw_output: String,
}

#[derive(Debug, Clone)]
pub struct BlameEntry {
    pub time_str: String,
    pub time_ms: u64,
    pub unit: String,
}

#[derive(Debug, Clone)]
pub struct JournalEntry {
    pub timestamp: String,
    pub hostname: String,
    pub unit: String,
    pub message: String,
    pub priority: u8,
}

#[derive(Debug, Clone)]
pub struct SystemInfo {
    pub hostname: String,
    pub kernel: String,
    pub uptime: String,
    pub systemd_version: String,
}

pub fn list_services() -> Vec<ServiceInfo> {
    let output = Command::new("systemctl")
        .args(["list-units", "--type=service", "--all", "--no-pager", "--no-legend", "--plain"])
        .output();

    let output = match output {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut services = Vec::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.splitn(5, char::is_whitespace).collect();
        let parts: Vec<&str> = parts.iter().map(|s| s.trim()).filter(|s| !s.is_empty()).collect();

        if parts.len() >= 4 {
            // Re-parse more carefully: split into whitespace-delimited tokens
            let tokens: Vec<&str> = line.split_whitespace().collect();
            if tokens.len() >= 4 {
                let name = tokens[0].trim_start_matches('●').trim().to_string();
                let load_state = tokens[1].to_string();
                let active_state = tokens[2].to_string();
                let sub_state = tokens[3].to_string();
                let description = if tokens.len() > 4 {
                    tokens[4..].join(" ")
                } else {
                    String::new()
                };

                services.push(ServiceInfo {
                    name,
                    load_state,
                    active_state,
                    sub_state,
                    description,
                });
            }
        }
    }

    services
}

pub fn get_boot_time() -> Option<BootTime> {
    let output = Command::new("systemd-analyze")
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    if !output.status.success() {
        return None;
    }

    let mut kernel = String::new();
    let mut userspace = String::new();
    let mut total = String::new();
    let mut firmware = None;
    let mut loader = None;

    // Parse lines like:
    // Startup finished in 3.456s (kernel) + 12.345s (userspace) = 15.801s
    // or with firmware/loader
    for line in stdout.lines() {
        if line.contains("Startup finished") {
            total = extract_after_equals(line).unwrap_or_default();

            if let Some(k) = extract_parenthesized(line, "kernel") {
                kernel = k;
            }
            if let Some(u) = extract_parenthesized(line, "userspace") {
                userspace = u;
            }
            if let Some(f) = extract_parenthesized(line, "firmware") {
                firmware = Some(f);
            }
            if let Some(l) = extract_parenthesized(line, "loader") {
                loader = Some(l);
            }
        }
    }

    Some(BootTime {
        kernel,
        userspace,
        total,
        firmware,
        loader,
        raw_output: stdout,
    })
}

fn extract_after_equals(line: &str) -> Option<String> {
    let idx = line.find('=')?;
    Some(line[idx + 1..].trim().to_string())
}

fn extract_parenthesized(line: &str, label: &str) -> Option<String> {
    let pattern = format!("({})", label);
    let idx = line.find(&pattern)?;
    // Walk backwards from idx to find the time value
    let before = &line[..idx].trim_end();
    let time_start = before.rfind(|c: char| c == '+' || c == ' ' || c == '\t')
        .map(|i| i + 1)
        .unwrap_or(0);
    Some(before[time_start..].trim().to_string())
}

pub fn get_blame(limit: usize) -> Vec<BlameEntry> {
    let output = Command::new("systemd-analyze")
        .args(["blame", "--no-pager"])
        .output();

    let output = match output {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut entries = Vec::new();

    for line in stdout.lines().take(limit) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.len() >= 2 {
            let time_str = tokens[0].to_string();
            let unit = tokens[tokens.len() - 1].to_string();
            let time_ms = parse_time_to_ms(&time_str);

            entries.push(BlameEntry {
                time_str,
                time_ms,
                unit,
            });
        }
    }

    entries
}

fn parse_time_to_ms(s: &str) -> u64 {
    // Parse strings like "5.123s", "123ms", "1min 2.345s"
    let s = s.trim();
    let mut total_ms: u64 = 0;

    if s.contains("min") {
        // Handle "1min 2.345s" format
        if let Some(min_idx) = s.find("min") {
            if let Ok(mins) = s[..min_idx].trim().parse::<f64>() {
                total_ms += (mins * 60_000.0) as u64;
            }
            let rest = s[min_idx + 3..].trim();
            if rest.ends_with('s') {
                if let Ok(secs) = rest[..rest.len() - 1].parse::<f64>() {
                    total_ms += (secs * 1000.0) as u64;
                }
            }
        }
    } else if s.ends_with("ms") {
        if let Ok(ms) = s[..s.len() - 2].parse::<f64>() {
            total_ms = ms as u64;
        }
    } else if s.ends_with('s') {
        if let Ok(secs) = s[..s.len() - 1].parse::<f64>() {
            total_ms = (secs * 1000.0) as u64;
        }
    }

    total_ms
}

pub fn get_critical_chain() -> Vec<String> {
    let output = Command::new("systemd-analyze")
        .args(["critical-chain", "--no-pager"])
        .output();

    let output = match output {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.lines().map(|l| l.to_string()).collect()
}

pub fn get_journal(lines: usize, priority: Option<u8>, unit: Option<&str>) -> Vec<JournalEntry> {
    let mut cmd = Command::new("journalctl");
    cmd.args(["--no-pager", "-o", "short", "--lines", &lines.to_string()]);

    if let Some(p) = priority {
        cmd.args(["-p", &p.to_string()]);
    }

    if let Some(u) = unit {
        cmd.args(["-u", u]);
    }

    let output = match cmd.output() {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut entries = Vec::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("-- ") {
            continue;
        }

        // Format: "May 07 12:34:56 hostname unit[pid]: message"
        // We'll do a best-effort parse
        let entry = parse_journal_line(line);
        entries.push(entry);
    }

    entries
}

fn parse_journal_line(line: &str) -> JournalEntry {
    // Try to split: "Mon DD HH:MM:SS hostname unit[pid]: message"
    // or "Mon DD HH:MM:SS hostname unit: message"
    let tokens: Vec<&str> = line.splitn(6, char::is_whitespace).collect();

    if tokens.len() >= 5 {
        let timestamp = format!("{} {} {}", tokens[0], tokens[1], tokens[2]);
        let hostname = tokens[3].to_string();

        let rest = if tokens.len() >= 6 {
            tokens[4..].join(" ")
        } else {
            tokens[4].to_string()
        };

        let (unit, message) = if let Some(colon_idx) = rest.find(':') {
            let u = rest[..colon_idx].to_string();
            let m = rest[colon_idx + 1..].trim().to_string();
            // Strip [pid] from unit
            let u = if let Some(bracket) = u.find('[') {
                u[..bracket].to_string()
            } else {
                u
            };
            (u, m)
        } else {
            (String::new(), rest)
        };

        let priority = guess_priority(&message);
        JournalEntry {
            timestamp,
            hostname,
            unit,
            message,
            priority,
        }
    } else {
        JournalEntry {
            timestamp: String::new(),
            hostname: String::new(),
            unit: String::new(),
            message: line.to_string(),
            priority: 6,
        }
    }
}

fn guess_priority(msg: &str) -> u8 {
    let lower = msg.to_lowercase();
    if lower.contains("emerg") || lower.contains("panic") {
        0
    } else if lower.contains("alert") {
        1
    } else if lower.contains("crit") {
        2
    } else if lower.contains("error") || lower.contains("fail") {
        3
    } else if lower.contains("warn") {
        4
    } else if lower.contains("notice") {
        5
    } else if lower.contains("debug") {
        7
    } else {
        6 // info
    }
}

pub fn get_unit_journal(unit: &str, lines: usize) -> Vec<String> {
    let output = Command::new("journalctl")
        .args(["-u", unit, "--no-pager", "-n", &lines.to_string()])
        .output();

    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            stdout.lines().map(|l| l.to_string()).collect()
        }
        Err(_) => vec!["Failed to read journal".to_string()],
    }
}

pub fn get_system_info() -> SystemInfo {
    let hostname = Command::new("hostname")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    let kernel = Command::new("uname")
        .arg("-r")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    let uptime = Command::new("uptime")
        .arg("-p")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    let systemd_version = Command::new("systemctl")
        .arg("--version")
        .output()
        .map(|o| {
            let s = String::from_utf8_lossy(&o.stdout);
            s.lines().next().unwrap_or("unknown").to_string()
        })
        .unwrap_or_else(|_| "unknown".to_string());

    SystemInfo {
        hostname,
        kernel,
        uptime,
        systemd_version,
    }
}

pub fn service_action(unit: &str, action: &str) -> Result<String, String> {
    let output = Command::new("systemctl")
        .args([action, unit])
        .output()
        .map_err(|e| format!("Failed to execute: {}", e))?;

    if output.status.success() {
        Ok(format!("Successfully {}ed {}", action, unit))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("Failed to {} {}: {}", action, unit, stderr.trim()))
    }
}
