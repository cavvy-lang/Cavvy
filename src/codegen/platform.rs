/// Windows 控制台 UTF-8 代码页
const UTF8_CODEPAGE: i32 = 65001;

/// 平台配置
#[derive(Debug, Clone)]
pub struct PlatformConfig {
    pub target_os: String,
    pub features: Vec<String>,
    pub no_features: Vec<String>,
    pub defines: Vec<String>,
    pub undefines: Vec<String>,
    pub obfuscate: bool,
    /// 启用 Rc<T> 循环引用运行时检测（--detect-cycles）。
    pub detect_cycles: bool,
    /// --no-panic：panic()/abort() 调用在代码生成阶段报编译错误。
    pub no_panic: bool,
}

impl PlatformConfig {
    pub fn new(target_os: &str) -> Self {
        Self {
            target_os: target_os.to_string(),
            features: Vec::new(),
            no_features: Vec::new(),
            defines: Vec::new(),
            undefines: Vec::new(),
            obfuscate: false,
            detect_cycles: false,
            no_panic: false,
        }
    }

    /// 检查特性是否启用
    pub fn is_feature_enabled(&self, feature: &str) -> bool {
        self.features.iter().any(|f| f == feature) && !self.no_features.iter().any(|f| f == feature)
    }

    /// 检查宏是否定义
    pub fn is_defined(&self, macro_name: &str) -> bool {
        self.defines.iter().any(|d| {
            // 支持 "NAME" 和 "NAME=value" 两种形式
            let name = d.split('=').next().unwrap_or(d).trim();
            name == macro_name
        }) && !self.undefines.iter().any(|d| {
            let name = d.split('=').next().unwrap_or(d).trim();
            name == macro_name
        })
    }

    /// 生成平台特定的运行时声明
    pub fn generate_platform_declarations(&self) -> String {
        let mut declarations = String::new();

        // Rc 循环引用检测运行时函数声明
        if self.detect_cycles {
            declarations.push_str("declare void @__cay_rc_set_detect(i32)\n");
            declarations.push_str("declare void @__cay_rc_register(i8*, i8*)\n");
            declarations.push_str("declare void @__cay_rc_unregister(i8*)\n");
            declarations.push_str("declare void @__cay_rc_edge_add(i8*, i8*)\n");
            declarations.push_str("declare void @__cay_rc_check_cycle(i8*)\n");
        }

        match self.target_os.as_str() {
            "windows" => {
                if self.is_feature_enabled("console_utf8") {
                    declarations.push_str("declare dllimport void @SetConsoleOutputCP(i32)\n");
                }
                if self.is_defined("WINDOWS_SPECIFIC") {
                    declarations.push_str("declare void @WindowsSpecificInit()\n");
                }
            }
            "linux" | "macos" => {
                if self.is_feature_enabled("console_utf8") {
                    declarations.push_str("declare i8* @setlocale(i32, i8*)\n");
                    declarations.push_str(
                        "@.str.locale = private unnamed_addr constant [6 x i8] c\"C.UTF-8\"\00\n",
                    );
                }
                if self.is_defined("LINUX_SPECIFIC") {
                    declarations.push_str("declare void @LinuxSpecificInit()\n");
                }
                if self.is_defined("MACOS_SPECIFIC") {
                    declarations.push_str("declare void @MacOSSpecificInit()\n");
                }
            }
            _ => {}
        }

        declarations
    }

    /// 生成平台特定的初始化代码
    pub fn generate_platform_init(&self) -> String {
        let mut code = String::new();

        // 在 main 入口最开头启用 Rc 循环引用检测
        if self.detect_cycles {
            code.push_str("  call void @__cay_rc_set_detect(i32 1)\n");
        }

        match self.target_os.as_str() {
            "windows" => {
                if self.is_feature_enabled("console_utf8") {
                    code.push_str(&format!(
                        "  call void @SetConsoleOutputCP(i32 {})\n",
                        UTF8_CODEPAGE
                    ));
                }
                if self.is_defined("WINDOWS_SPECIFIC") {
                    code.push_str("  call void @WindowsSpecificInit()\n");
                }
            }
            "linux" | "macos" => {
                if self.is_feature_enabled("console_utf8") {
                    code.push_str("  call i8* @setlocale(i32 0, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @.str.locale, i32 0, i32 0))\n");
                }
                if self.is_defined("LINUX_SPECIFIC") {
                    code.push_str("  call void @LinuxSpecificInit()\n");
                }
                if self.is_defined("MACOS_SPECIFIC") {
                    code.push_str("  call void @MacOSSpecificInit()\n");
                }
            }
            _ => {}
        }

        code
    }
}

/// 平台特定代码生成器
pub struct PlatformCodeGenerator {
    config: PlatformConfig,
}

impl PlatformCodeGenerator {
    pub fn new(config: PlatformConfig) -> Self {
        Self { config }
    }

    /// 生成平台特定的初始化代码
    pub fn generate_platform_init(&self) -> String {
        let mut code = String::new();

        if self.config.detect_cycles {
            code.push_str("  ; Enable Rc cycle detection\n");
            code.push_str("  call void @__cay_rc_set_detect(i32 1)\n");
        }

        match self.config.target_os.as_str() {
            "windows" => {
                if self.config.is_feature_enabled("console_utf8") {
                    code.push_str("  ; Windows UTF-8 console setup\n");
                    code.push_str(&format!(
                        "  call void @SetConsoleOutputCP(i32 {})\n",
                        UTF8_CODEPAGE
                    ));
                }

                if self.config.is_defined("WINDOWS_SPECIFIC") {
                    code.push_str("  ; Windows-specific initialization\n");
                    code.push_str("  call void @WindowsSpecificInit()\n");
                }
            }
            "linux" => {
                if self.config.is_feature_enabled("console_utf8") {
                    code.push_str("  ; Linux UTF-8 locale setup\n");
                    code.push_str("  %locale_ptr = call i8* @setlocale(i32 0, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @.str.locale, i32 0, i32 0))\n");
                }

                if self.config.is_defined("LINUX_SPECIFIC") {
                    code.push_str("  ; Linux-specific initialization\n");
                    code.push_str("  call void @LinuxSpecificInit()\n");
                }
            }
            "macos" => {
                if self.config.is_feature_enabled("console_utf8") {
                    code.push_str("  ; macOS UTF-8 locale setup\n");
                    code.push_str("  %locale_ptr = call i8* @setlocale(i32 0, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @.str.locale, i32 0, i32 0))\n");
                }

                if self.config.is_defined("MACOS_SPECIFIC") {
                    code.push_str("  ; macOS-specific initialization\n");
                    code.push_str("  call void @MacOSSpecificInit()\n");
                }
            }
            _ => {
                code.push_str("  ; Generic platform initialization\n");
            }
        }

        code
    }

    /// 生成平台特定的运行时声明
    pub fn generate_platform_declarations(&self) -> String {
        let mut declarations = String::new();

        declarations.push_str("; Platform-specific runtime declarations\n");

        match self.config.target_os.as_str() {
            "windows" => {
                declarations.push_str("declare dllimport void @SetConsoleOutputCP(i32)\n");
                if self.config.is_defined("WINDOWS_SPECIFIC") {
                    declarations.push_str("declare void @WindowsSpecificInit()\n");
                }
            }
            "linux" => {
                declarations.push_str("declare i8* @setlocale(i32, i8*)\n");
                declarations.push_str(
                    "@.str.locale = private unnamed_addr constant [6 x i8] c\\\"C.UTF-8\\\"\\00\n",
                );
                if self.config.is_defined("LINUX_SPECIFIC") {
                    declarations.push_str("declare void @LinuxSpecificInit()\n");
                }
            }
            "macos" => {
                declarations.push_str("declare i8* @setlocale(i32, i8*)\n");
                declarations.push_str(
                    "@.str.locale = private unnamed_addr constant [6 x i8] c\\\"C.UTF-8\\\"\\00\n",
                );
                if self.config.is_defined("MACOS_SPECIFIC") {
                    declarations.push_str("declare void @MacOSSpecificInit()\n");
                }
            }
            _ => {}
        }

        declarations
    }

    /// 生成跨平台的动态链接库加载代码
    pub fn generate_dynamic_library_loading(&self) -> String {
        let mut code = String::new();

        code.push_str("; Dynamic library loading support\n");

        match self.config.target_os.as_str() {
            "windows" => {
                code.push_str("declare i8* @LoadLibraryA(i8*)\n");
                code.push_str("declare i8* @GetProcAddress(i8*, i8*)\n");
                code.push_str("declare i32 @FreeLibrary(i8*)\n");
            }
            "linux" | "macos" => {
                code.push_str("declare i8* @dlopen(i8*, i32)\n");
                code.push_str("declare i8* @dlsym(i8*, i8*)\n");
                code.push_str("declare i32 @dlclose(i8*)\n");
            }
            _ => {}
        }

        code
    }
}
