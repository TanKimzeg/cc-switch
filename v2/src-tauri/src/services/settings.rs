//! 应用行为设置（窗口/托盘/自启）的键定义与读取。

use crate::db::Database;

pub const KEY_SHOW_IN_TRAY: &str = "app.showInTray";
pub const KEY_MINIMIZE_TO_TRAY_ON_CLOSE: &str = "app.minimizeToTrayOnClose";
pub const KEY_SILENT_STARTUP: &str = "app.silentStartup";
pub const KEY_LAUNCH_ON_STARTUP: &str = "app.launchOnStartup";

/// 应用行为设置快照（对齐 v1 AppSettings 的设备级行为字段子集）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppBehavior {
    pub show_in_tray: bool,
    pub minimize_to_tray_on_close: bool,
    pub silent_startup: bool,
    pub launch_on_startup: bool,
}

impl Default for AppBehavior {
    fn default() -> Self {
        Self {
            show_in_tray: true,
            minimize_to_tray_on_close: true,
            silent_startup: false,
            launch_on_startup: false,
        }
    }
}

impl Database {
    /// 读取布尔设置：值恰为 "1" 时为 true，缺省/其他取默认。
    pub fn get_bool_setting(&self, key: &str, default: bool) -> bool {
        match self.get_setting(key) {
            Ok(Some(v)) => v == "1",
            _ => default,
        }
    }

    pub fn set_bool_setting(&self, key: &str, value: bool) -> rusqlite::Result<()> {
        self.set_setting(key, if value { "1" } else { "0" })
    }

    pub fn get_app_behavior(&self) -> AppBehavior {
        let d = AppBehavior::default();
        AppBehavior {
            show_in_tray: self.get_bool_setting(KEY_SHOW_IN_TRAY, d.show_in_tray),
            minimize_to_tray_on_close: self.get_bool_setting(
                KEY_MINIMIZE_TO_TRAY_ON_CLOSE,
                d.minimize_to_tray_on_close,
            ),
            silent_startup: self.get_bool_setting(KEY_SILENT_STARTUP, d.silent_startup),
            launch_on_startup: self.get_bool_setting(KEY_LAUNCH_ON_STARTUP, d.launch_on_startup),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn behavior_defaults_and_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(&dir.path().join("cc.db")).unwrap();

        let b = db.get_app_behavior();
        assert!(b.show_in_tray);
        assert!(b.minimize_to_tray_on_close);
        assert!(!b.silent_startup);
        assert!(!b.launch_on_startup);

        db.set_bool_setting(KEY_SHOW_IN_TRAY, false).unwrap();
        db.set_bool_setting(KEY_SILENT_STARTUP, true).unwrap();
        db.set_bool_setting(KEY_LAUNCH_ON_STARTUP, true).unwrap();

        let b = db.get_app_behavior();
        assert!(!b.show_in_tray);
        assert!(b.minimize_to_tray_on_close);
        assert!(b.silent_startup);
        assert!(b.launch_on_startup);
    }

    #[test]
    fn bool_setting_treats_non_one_as_false() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(&dir.path().join("cc.db")).unwrap();
        db.set_setting("x.flag", "0").unwrap();
        assert!(!db.get_bool_setting("x.flag", true));
        db.set_setting("x.flag", "garbage").unwrap();
        assert!(!db.get_bool_setting("x.flag", true));
        db.set_setting("x.flag", "1").unwrap();
        assert!(db.get_bool_setting("x.flag", false));
    }
}
