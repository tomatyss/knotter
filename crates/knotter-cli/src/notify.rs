use anyhow::Result;

pub trait Notifier {
    fn send(&self, title: &str, body: &str) -> Result<()>;
}

pub struct StdoutNotifier;

impl Notifier for StdoutNotifier {
    fn send(&self, title: &str, body: &str) -> Result<()> {
        println!("{title}: {body}");
        Ok(())
    }
}

#[cfg(feature = "desktop-notify")]
pub struct DesktopNotifier;

#[cfg(feature = "desktop-notify")]
impl Notifier for DesktopNotifier {
    fn send(&self, title: &str, body: &str) -> Result<()> {
        notify_rust::Notification::new()
            .summary(title)
            .body(body)
            .show()?;
        Ok(())
    }
}
