//! 测试共享工具：跨模块串行化环境变量相关的测试。

use std::sync::{Mutex, OnceLock};

/// 全局环境变量锁。
///
/// 设置/读取 `CC_SWITCH_TEST_HOME`、`XDG_DATA_HOME`、`CC_SWITCH_OPENCODE_DATA_DIR`
/// 等全局环境变量的测试必须持有此锁，避免并行执行时互相污染。
pub fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}
