use crate::source::VcfSource;
use crate::{Result, SyncError};

#[derive(Debug, Clone)]
pub struct MacosContactsSource {
    pub group: Option<String>,
}

impl MacosContactsSource {
    pub fn new(group: Option<String>) -> Self {
        Self { group }
    }
}

impl VcfSource for MacosContactsSource {
    fn source_name(&self) -> &'static str {
        "macos-contacts"
    }

    fn fetch_vcf(&self) -> Result<String> {
        fetch_contacts_vcf(self.group.as_deref())
    }
}

#[cfg(target_os = "macos")]
fn fetch_contacts_vcf(group: Option<&str>) -> Result<String> {
    use std::process::Command;

    let script = r#"
on run argv
    set oldDelimiters to AppleScript's text item delimiters
    set AppleScript's text item delimiters to linefeed
    set cards to {}
    set succeeded to false
    repeat 5 times
        try
            tell application "Contacts"
                if (count of argv) is 0 then
                    set cards to vcard of people
                else
                    set targetGroup to item 1 of argv
                    set targetGroupRef to first group whose name is targetGroup
                    set cards to vcard of people of targetGroupRef
                end if
            end tell
            set succeeded to true
            exit repeat
        on error errMsg number errNum
            if errNum is -600 then
                tell application "Contacts" to launch
                delay 0.2
            else
                error errMsg number errNum
            end if
        end try
    end repeat
    if succeeded is false then
        error "Contacts did not respond" number -600
    end if
    if (count of cards) is 0 then
        set joined to ""
    else
        set joined to cards as text
    end if
    set AppleScript's text item delimiters to oldDelimiters
    return joined
end run
"#;

    let mut cmd = Command::new("osascript");
    cmd.arg("-e").arg(script);
    if let Some(group) = group {
        if !group.trim().is_empty() {
            cmd.arg(group);
        }
    }

    let output = cmd.output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = if stderr.trim().is_empty() {
            format!("osascript exited with status {}", output.status)
        } else {
            stderr.trim().to_string()
        };
        return Err(SyncError::Command(message));
    }

    String::from_utf8(output.stdout)
        .map_err(|_| SyncError::Parse("macOS Contacts output was not valid UTF-8".to_string()))
}

#[cfg(not(target_os = "macos"))]
fn fetch_contacts_vcf(_group: Option<&str>) -> Result<String> {
    Err(SyncError::Unavailable(
        "macOS Contacts import is only available on macOS".to_string(),
    ))
}
