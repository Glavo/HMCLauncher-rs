use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;
use windows_sys::core::{PCWSTR, w};

/// Bundle the small set of localized launcher error messages.
pub struct I18n {
    pub error_self_path: PCWSTR,
    pub error_invalid_hmcl_java_home: PCWSTR,
    pub error_java_not_found: PCWSTR,
}

/// Pick the small built-in message set that matches the user's UI language.
pub fn current() -> I18n {
    if unsafe { GetUserDefaultUILanguage() } == 2052 {
        I18n {
            error_self_path: w!("获取程序路径失败。"),
            error_invalid_hmcl_java_home: w!(
                "HMCL_JAVA_HOME 所指向的 Java 路径无效，请更新或删除该变量。\n"
            ),
            error_java_not_found: w!(
                "HMCL 需要 Java 17 或更高版本才能运行，点击“确定”开始下载 Java。\n请在安装 Java 完成后重新启动 HMCL。"
            ),
        }
    } else {
        I18n {
            error_self_path: w!("Failed to get the exe path."),
            error_invalid_hmcl_java_home: w!(
                "The Java path specified by HMCL_JAVA_HOME is invalid. Please update it to a valid Java installation path or remove this environment variable."
            ),
            error_java_not_found: w!(
                "HMCL requires Java 17 or later to run,\nClick 'OK' to start downloading java.\nPlease restart HMCL after installing Java."
            ),
        }
    }
}
